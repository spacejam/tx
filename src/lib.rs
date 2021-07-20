//! `tx`, a transaction system based a bit on
//! [Cicada](https://15721.courses.cs.cmu.edu/spring2018/papers/06-mvcc2/lim-sigmod2017.pdf)
//! but using batched txid allocations instead of
//! the fast-burning technique in the paper.
//!
//! read phase:
//! * generate unique txid
//! * accumulate viewed Cows in a local read cache
//!     * possibly early-abort if conflicts are detected
//!
//! validation phase:
//! * sort write set by contention
//! * pre-check version consistency
//! * install pending versions
//! * update read timestamp
//! * check version consistency
//!
//! write phase:
//! * log writeset
//! * commit pending versions
//!
//! transaction is now committed
//!
//! maintenance (can be delayed a bit based on memory vs cpu trade-off):
//! * schedule gc of replaced versions
//! * declare quiescent state
//! * collect garbage created by past transactions

pub mod ebr;
pub mod rand;
pub mod stack;
pub mod tx;
