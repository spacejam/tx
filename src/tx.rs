use std::{
    borrow::Cow,
    collections::HashMap,
    sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering::SeqCst},
};

use crate::ebr::Ebr;

const CONTENTION_THRESHOLD: usize = 1;

static TX: AtomicU64 = AtomicU64::new(0);

struct VersionChain<T: Clone + Send + 'static> {
    head: AtomicPtr<Version<T>>,
}

struct Version<T: Clone + Send + 'static> {
    item: T,
    wts: AtomicU64,
    rts: AtomicU64,
}

#[derive(Debug)]
pub struct TransactionalSystem<T: Clone + Send + 'static> {
    ebr: Ebr<T>,
}

impl<T: Clone + Send + 'static> TransactionalSystem<T> {
    pub fn handle(&self) -> LocalHandle<T> {
        LocalHandle {
            system: self,
            aborts: 0,
        }
    }
}

#[derive(Debug)]
pub struct LocalHandle<'a, T: Clone + Send + 'static> {
    system: &'a TransactionalSystem<T>,
    aborts: usize,
}

impl<'a, T: Clone + Send + 'static> LocalHandle<'a, T> {
    pub fn tx(&'a mut self) -> Transaction<'a, T> {
        Transaction {
            handle: self,
            cache: Default::default(),
            has_aborted: false,
            ts: TX.fetch_add(1, SeqCst),
        }
    }

    fn is_contended(&self) -> bool {
        self.aborts >= CONTENTION_THRESHOLD
    }
}

#[derive(Debug)]
pub struct Transaction<'a, T: Clone + Send + 'static> {
    handle: &'a mut LocalHandle<'a, T>,
    cache: HashMap<usize, Cow<'a, T>>,
    ts: u64,
    has_aborted: bool,
}

impl<'a, T: Clone + Send + 'static> Transaction<'a, T> {
    pub fn tx<F, R>(&mut self, mut f: F) -> R
    where
        F: FnMut() -> R,
    {
        let res = loop {
            let result = f();

            if self.try_commit() {
                break result;
            }

            self.maybe_bump_aborts();
        };

        self.maybe_decr_aborts();

        res
    }

    fn maybe_bump_aborts(&mut self) {
        if self.has_aborted {
            self.handle.aborts = self.handle.aborts.min(4) + 1;
        }
    }

    fn maybe_decr_aborts(&mut self) {
        if self.has_aborted {
            self.handle.aborts = self.handle.aborts.max(1) - 1;

            self.has_aborted = false;
        }
    }

    pub fn try_commit(&mut self) -> bool {
        /*
        // clear out local cache
        let transacted = L
            .with(|l| {
                let lr: &mut HashMap<_, _> = &mut *l.borrow_mut();
                std::mem::replace(lr, HashMap::new())
            })
            .into_iter()
            .map(|(tvar, local)| {
                let (rts, vsn_ptr) = unsafe {
                    G.get(tvar, &guard)
                        .expect("should find TVar in global lookup table")
                        .deref()
                };
                (tvar, rts, vsn_ptr, local)
            })
            .collect::<Vec<_>>();

        // install pending
        for (_tvar, _rts, vsn_ptr, local) in transacted
            .iter()
            .filter(|(_, _, _, local)| local.is_write())
        {
            let current_ptr = vsn_ptr.load(SeqCst, &guard);
            let current = unsafe { current_ptr.deref() };

            if !current.pending.is_null()
                || current.pending_wts != 0
                || current.stable_wts > ts
                || local.read_wts() > ts
            {
                // write conflict
                return cleanup(ts, transacted, &guard);
            }

            let mut new = current.clone();
            new.pending_wts = ts;
            new.pending = local.ptr();

            match vsn_ptr.compare_and_set(
                current_ptr,
                Owned::new(new).into_shared(&guard),
                SeqCst,
                &guard,
            ) {
                Ok(_old) => unsafe {
                    guard.defer_destroy(current_ptr);
                },
                Err(_) => {
                    // write conflict
                    return cleanup(ts, transacted, &guard);
                }
            }
        }

        // update rts
        for (_tvar, rts, vsn_ptr, local) in transacted.iter() {
            bump_gte(rts, ts);

            let current_ptr = vsn_ptr.load(SeqCst, &guard);
            let current = unsafe { current_ptr.deref() };

            if current.stable_wts > ts {
                return cleanup(ts, transacted, &guard);
            }

            let pending_predecessor = current.pending_wts != 0 && current.pending_wts < ts;
            if current.stable_wts != local.read_wts() || pending_predecessor {
                return cleanup(ts, transacted, &guard);
            }
        }

        // check version consistency
        for (_tvar, rts, vsn_ptr, local) in transacted
            .iter()
            .filter(|(_, _, _, local)| local.is_write())
        {
            let rts = rts.load(SeqCst);

            if rts > ts {
                // read conflict
                return cleanup(ts, transacted, &guard);
            }

            let current = unsafe { vsn_ptr.load(SeqCst, &guard).deref() };

            assert_eq!(
                current.pending_wts, ts,
                "somehow our pending write got lost"
            );

            if current.stable_wts != local.read_wts() {
                // write conflict
                return cleanup(ts, transacted, &guard);
            }
        }

        // commit
        for (_tvar, _rts, vsn_ptr, local) in transacted
            .iter()
            .filter(|(_, _, _, local)| local.is_write())
        {
            let current_ptr = vsn_ptr.load(SeqCst, &guard);
            let current = unsafe { current_ptr.deref() };

            assert_eq!(
                current.pending_wts, ts,
                "somehow our pending write got lost"
            );

            if current.pending.is_null() {
                panic!("somehow an item in our commit writeset isn't present in the Vsn anymore");
            }

            let new = Owned::new(Vsn {
                stable: current.pending,
                stable_wts: current.pending_wts,
                pending: std::ptr::null_mut(),
                pending_wts: 0,
            });

            match vsn_ptr.compare_and_set(current_ptr, new, SeqCst, &guard) {
                Ok(_old) => {
                    unsafe {
                        guard.defer_destroy(current_ptr);
                    }

                    // handle GC
                    let drop_container = DropContainer(current.stable as usize);
                    let dropper = local.dropper().0;
                    guard.defer(move || {
                        let dropper: fn(DropContainer) = unsafe { std::mem::transmute(dropper) };
                        dropper(drop_container)
                    });
                }
                Err(_) => {
                    // write conflict
                    panic!("somehow we got a conflict while committing a transaction");
                }
            }
        }

        TS.with(|ts| *ts.borrow_mut() = 0);
        GUARD.with(|g| {
            g.borrow_mut()
                .take()
                .expect("should be able to end transaction")
        });
        */

        true
    }

    fn cleanup(&mut self) {}
}

#[derive(Clone, Debug)]
struct Vsn {
    stable_wts: usize,
    pending_wts: usize,
}

fn bump_gte(a: &AtomicUsize, to: usize) {
    let mut current = a.load(SeqCst);
    while current < to as usize {
        match a.compare_exchange(current, to, SeqCst, SeqCst) {
            Ok(_) => return,
            Err(c) => current = c,
        }
    }
}
