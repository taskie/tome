//! Parallel streaming encryption/decryption using a crossbeam pipeline.
//!
//! Architecture: Reader thread → N worker threads → Writer (main thread).
//!
//! Each chunk is independently encrypted/decrypted using a deterministic nonce
//! derived from (base_nonce, chunk_index, is_last). The writer reassembles
//! chunks in order using a BTreeMap reorder buffer.
//!
//! Buffer recycling: a return channel (`ret_tx` / `ret_rx`) circulates
//! pre-allocated `Vec<u8>` buffers so the steady-state allocation count is zero.
//!
//! # Safety note (decryption)
//!
//! Because chunks are decrypted and written independently, plaintext is emitted
//! before the final chunk's AEAD tag has been verified. If the stream is
//! truncated or tampered with, an error is returned after partial output has
//! already been written. Callers should write to a temporary location and only
//! commit the output after a successful return.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam::channel::{Receiver, Sender, bounded};

use crate::algorithm::CipherAlgorithm;
use crate::cipher::{AeadInner, read_exact_or_eof};
use crate::error::{AetherError, Result};
use crate::header::{ChunkKind, KEY_SIZE, MAX_NONCE_SIZE, compute_nonce};

const TAG_SIZE: usize = 16;

// ──────────────────────────────────────────────────────────────────────────────
// Internal types
// ──────────────────────────────────────────────────────────────────────────────

/// Shared parameters for parallel stream operations.
pub(crate) struct ParStreamParams<'a> {
    pub algorithm: CipherAlgorithm,
    pub key: &'a [u8; KEY_SIZE],
    pub nonce_original: &'a [u8; MAX_NONCE_SIZE],
    pub nonce_size: usize,
    pub chunk_kind: ChunkKind,
    pub first_chunk_ad: &'a [u8],
    pub num_workers: usize,
}

struct ChunkJob {
    index: u64,
    /// Recycled buffer containing chunk data in `[0..len]`.
    buf: Vec<u8>,
    len: usize,
    is_last: bool,
}

struct ProcessedChunk {
    index: u64,
    /// On success: `Ok((buf, len))` where `buf[0..len]` is the result.
    /// The `buf` is returned to the pool after writing.
    data: Result<(Vec<u8>, usize)>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Public (crate) API
// ──────────────────────────────────────────────────────────────────────────────

fn resolve_workers(num_workers: usize) -> usize {
    if num_workers == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) } else { num_workers }
}

/// Parallel STREAM encryption.
///
/// Reads plaintext chunks from `r`, encrypts them across `num_workers` threads,
/// and writes ciphertext to `w` in order.
pub(crate) fn par_stream_encrypt<R: BufRead + Send, W: Write>(
    r: &mut R,
    w: &mut W,
    p: &ParStreamParams<'_>,
) -> Result<()> {
    let num_workers = resolve_workers(p.num_workers);
    let pt_size = p.chunk_kind.plaintext_size();
    let buf_capacity = pt_size + TAG_SIZE;
    let pool_size = num_workers * 2;
    let reader_err: Mutex<Option<AetherError>> = Mutex::new(None);
    let cancelled = AtomicBool::new(false);

    let scope_result = crossbeam::scope(|s| -> Result<()> {
        let (job_tx, job_rx) = bounded::<ChunkJob>(pool_size);
        let (result_tx, result_rx) = bounded::<ProcessedChunk>(pool_size);
        let (ret_tx, ret_rx) = bounded::<Vec<u8>>(pool_size);

        // Seed the return channel with pre-allocated buffers.
        for _ in 0..pool_size {
            ret_tx.send(Vec::with_capacity(buf_capacity)).unwrap();
        }

        // ── Reader thread ────────────────────────────────────────────────
        let reader_err_ref = &reader_err;
        let cancelled_ref = &cancelled;
        s.spawn(move |_| {
            if let Err(e) = read_chunks_encrypt(r, &job_tx, &ret_rx, pt_size, cancelled_ref) {
                *reader_err_ref.lock().unwrap() = Some(e);
            }
        });

        // ── Worker threads ───────────────────────────────────────────────
        for _ in 0..num_workers {
            let rx = job_rx.clone();
            let tx = result_tx.clone();
            let cancelled_ref = &cancelled;
            s.spawn(move |_| {
                let aead = AeadInner::new(p.algorithm, p.key);
                for job in rx {
                    let nonce = compute_nonce(p.nonce_original, p.nonce_size, job.index, job.is_last);
                    let ad = if job.index == 0 { p.first_chunk_ad } else { &[] };
                    let mut buf = job.buf;
                    buf.truncate(job.len);
                    let data = aead.encrypt_in_place(&nonce, &mut buf, ad).map(|()| {
                        let len = buf.len();
                        (buf, len)
                    });
                    let is_err = data.is_err();
                    if tx.send(ProcessedChunk { index: job.index, data }).is_err() {
                        break;
                    }
                    if is_err {
                        cancelled_ref.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            });
        }
        drop(job_rx);
        drop(result_tx);

        write_ordered(&result_rx, w, &ret_tx)
    });

    let write_result = scope_result.map_err(|_| AetherError::Encryption("parallel encryption panicked".into()))?;
    if let Some(e) = reader_err.into_inner().unwrap() {
        return Err(e);
    }
    write_result
}

/// Parallel STREAM decryption.
///
/// Reads ciphertext chunks from `r`, decrypts them across `num_workers` threads,
/// and writes plaintext to `w` in order.
///
/// **Plaintext is written before the final chunk is verified.** If authentication
/// of any chunk fails, the function returns an error, but earlier chunks may
/// have already been written.
pub(crate) fn par_stream_decrypt<R: BufRead + Send, W: Write>(
    r: &mut R,
    w: &mut W,
    p: &ParStreamParams<'_>,
) -> Result<()> {
    let num_workers = resolve_workers(p.num_workers);
    let ct_size = p.chunk_kind.ciphertext_size();
    let pool_size = num_workers * 2;
    let reader_err: Mutex<Option<AetherError>> = Mutex::new(None);
    let cancelled = AtomicBool::new(false);

    let scope_result = crossbeam::scope(|s| -> Result<()> {
        let (job_tx, job_rx) = bounded::<ChunkJob>(pool_size);
        let (result_tx, result_rx) = bounded::<ProcessedChunk>(pool_size);
        let (ret_tx, ret_rx) = bounded::<Vec<u8>>(pool_size);

        for _ in 0..pool_size {
            ret_tx.send(Vec::with_capacity(ct_size)).unwrap();
        }

        // ── Reader thread ────────────────────────────────────────────────
        let reader_err_ref = &reader_err;
        let cancelled_ref = &cancelled;
        s.spawn(move |_| {
            if let Err(e) = read_chunks_decrypt(r, &job_tx, &ret_rx, ct_size, cancelled_ref) {
                *reader_err_ref.lock().unwrap() = Some(e);
            }
        });

        // ── Worker threads ───────────────────────────────────────────────
        for _ in 0..num_workers {
            let rx = job_rx.clone();
            let tx = result_tx.clone();
            let cancelled_ref = &cancelled;
            s.spawn(move |_| {
                let aead = AeadInner::new(p.algorithm, p.key);
                for job in rx {
                    let nonce = compute_nonce(p.nonce_original, p.nonce_size, job.index, job.is_last);
                    let ad = if job.index == 0 { p.first_chunk_ad } else { &[] };
                    let mut buf = job.buf;
                    buf.truncate(job.len);
                    let data = aead.decrypt_in_place(&nonce, &mut buf, ad).map(|()| {
                        let len = buf.len();
                        (buf, len)
                    });
                    let is_err = data.is_err();
                    if tx.send(ProcessedChunk { index: job.index, data }).is_err() {
                        break;
                    }
                    if is_err {
                        cancelled_ref.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            });
        }
        drop(job_rx);
        drop(result_tx);

        write_ordered(&result_rx, w, &ret_tx)
    });

    let write_result = scope_result.map_err(|_| AetherError::Decryption("parallel decryption panicked".into()))?;
    if let Some(e) = reader_err.into_inner().unwrap() {
        return Err(e);
    }
    write_result
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Read plaintext chunks and send them to the job channel.
/// Acquires recycled buffers from `ret_rx` instead of allocating.
fn read_chunks_encrypt<R: BufRead>(
    r: &mut R,
    tx: &Sender<ChunkJob>,
    ret_rx: &Receiver<Vec<u8>>,
    pt_size: usize,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut index = 0u64;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let mut buf = match ret_rx.recv() {
            Ok(b) => b,
            Err(_) => break,
        };
        buf.resize(pt_size, 0);
        let pos = read_exact_or_eof(r, &mut buf[..pt_size])?;
        if pos == 0 && index > 0 {
            // Return unused buffer and exit.
            let _ = ret_rx.try_recv().ok(); // ignore — we already have `buf`
            break;
        }
        let is_last = pos < pt_size || r.fill_buf()?.is_empty();
        let job = ChunkJob { index, buf, len: pos, is_last };
        if tx.send(job).is_err() {
            break;
        }
        index += 1;
        if is_last {
            break;
        }
    }
    Ok(())
}

/// Read ciphertext chunks and send them to the job channel.
///
/// Uses read-ahead (`fill_buf`) to definitively detect the last chunk,
/// eliminating the need for the double-nonce trial used in serial decryption.
fn read_chunks_decrypt<R: BufRead>(
    r: &mut R,
    tx: &Sender<ChunkJob>,
    ret_rx: &Receiver<Vec<u8>>,
    ct_size: usize,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut index = 0u64;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let mut buf = match ret_rx.recv() {
            Ok(b) => b,
            Err(_) => break,
        };
        buf.resize(ct_size, 0);
        let pos = read_exact_or_eof(r, &mut buf[..ct_size])?;
        if pos == 0 {
            if index == 0 {
                let _ = tx.send(ChunkJob { index: 0, buf, len: 0, is_last: true });
            }
            break;
        }
        let is_last = pos < ct_size || r.fill_buf()?.is_empty();
        let job = ChunkJob { index, buf, len: pos, is_last };
        if tx.send(job).is_err() {
            break;
        }

        if is_last {
            let mut trail = [0u8; 1];
            let n = r.read(&mut trail)?;
            if n > 0 {
                return Err(AetherError::Decryption("data after last chunk".into()));
            }
            break;
        }
        index += 1;
    }
    Ok(())
}

/// Receive processed chunks and write them in order.
///
/// Uses a BTreeMap to reorder out-of-sequence results from the worker pool.
/// After writing, returns the buffer to the pool via `ret_tx`.
fn write_ordered<W: Write>(rx: &Receiver<ProcessedChunk>, w: &mut W, ret_tx: &Sender<Vec<u8>>) -> Result<()> {
    let mut next_index: u64 = 0;
    let mut pending: BTreeMap<u64, (Vec<u8>, usize)> = BTreeMap::new();

    for chunk in rx {
        let (buf, len) = chunk.data?;
        if chunk.index == next_index {
            w.write_all(&buf[..len])?;
            let _ = ret_tx.send(buf);
            next_index += 1;
            while let Some((pbuf, plen)) = pending.remove(&next_index) {
                w.write_all(&pbuf[..plen])?;
                let _ = ret_tx.send(pbuf);
                next_index += 1;
            }
        } else {
            pending.insert(chunk.index, (buf, len));
        }
    }
    Ok(())
}
