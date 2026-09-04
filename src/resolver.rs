//! Concurrent presigned-URL resolution.
//!
//! Each `(asset, coin, month)` partition needs its own short-lived presigned
//! URL, minted via the download endpoint. For multi-partition reads we resolve
//! them concurrently — a scoped thread pool for the sync client,
//! `futures::try_join_all` for async.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use futures::future::try_join_all;

use crate::error::TesseraError;
use crate::models::PartitionRef;
use crate::readers::ResolvedPartition;

/// Cap on worker threads / concurrent URL fetches.
pub const MAX_WORKERS: usize = 8;

/// Resolve presigned URLs for `refs` concurrently, preserving order.
///
/// A single ref is fetched inline; multiple refs are fanned out over at most
/// [`MAX_WORKERS`] worker threads with short-circuit on the first error.
///
/// # Errors
///
/// Returns the first fetch error, if any.
/// # Panics
///
/// Panics if internal bookkeeping is inconsistent (a slot is unresolved
/// although no error was recorded) — unreachable by construction.
pub fn resolve_sync(
    fetch_url: impl Fn(&PartitionRef) -> Result<String, TesseraError> + Sync,
    refs: &[PartitionRef],
) -> Result<Vec<ResolvedPartition>, TesseraError> {
    if let [partition] = refs {
        return Ok(vec![(partition.clone(), fetch_url(partition)?)]);
    }

    let workers = MAX_WORKERS.min(refs.len());
    let next_index = AtomicUsize::new(0);
    let urls: Mutex<Vec<Option<String>>> = Mutex::new(vec![None; refs.len()]);
    let first_error: Mutex<Option<TesseraError>> = Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if first_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .is_some()
                    {
                        break;
                    }
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    if index >= refs.len() {
                        break;
                    }
                    match fetch_url(&refs[index]) {
                        Ok(url) => {
                            urls.lock().unwrap_or_else(PoisonError::into_inner)[index] = Some(url);
                        }
                        Err(err) => {
                            let mut guard =
                                first_error.lock().unwrap_or_else(PoisonError::into_inner);
                            if guard.is_none() {
                                *guard = Some(err);
                            }
                            break;
                        }
                    }
                }
            });
        }
    });

    if let Some(err) = first_error
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
    {
        return Err(err);
    }
    let resolved = urls.into_inner().unwrap_or_else(PoisonError::into_inner);
    Ok(refs
        .iter()
        .zip(resolved)
        .map(|(partition, url)| {
            (
                partition.clone(),
                url.expect("resolved when no error recorded"),
            )
        })
        .collect())
}

/// Resolve presigned URLs for `refs` concurrently, preserving order.
///
/// # Errors
///
/// Returns the first fetch error, if any.
pub async fn resolve_async<F, Fut>(
    fetch_url: F,
    refs: &[PartitionRef],
) -> Result<Vec<ResolvedPartition>, TesseraError>
where
    F: Fn(PartitionRef) -> Fut,
    Fut: Future<Output = Result<String, TesseraError>>,
{
    let urls = try_join_all(refs.iter().map(|partition| fetch_url(partition.clone()))).await?;
    Ok(refs.iter().cloned().zip(urls).collect())
}
