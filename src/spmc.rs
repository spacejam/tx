use std::{
    ops::Deref,
    sync::{
        atomic::{
            AtomicPtr,
            Ordering::{Acquire, Release},
        },
        Arc,
    },
};

pub fn spmc<T: 'static + Send>() -> (Sender<T>, Receiver<T>) {
    todo!()
}

pub struct Sender<T: 'static + Send> {
    head: Arc<AtomicPtr<Node<T>>>,
}

impl<T: 'static + Send> Sender<T> {
    pub fn push(&mut self, item: T) {
        let cur_head_ptr = self.head();

        let node_ptr = Box::into_raw(Box::new(Node {
            item,
            next: cur_head_ptr.into(),
        }));

        self.head
            .compare_exchange(cur_head_ptr, node_ptr, Release, Release)
            .expect("single-producer queue should never fail to push");
    }

    fn head<'g>(&self) -> *mut Node<T> {
        self.head.load(Acquire)
    }
}

impl<T: 'static + Send> Receiver<T> {
    pub fn recv(&mut self, item: T) -> T {
        todo!()
    }

    fn head<'g>(&self) -> *mut Node<T> {
        self.head.load(Acquire)
    }
}

#[derive(Debug, Clone)]
pub struct Receiver<T: 'static + Send> {
    head: Arc<AtomicPtr<Node<T>>>,
}

unsafe impl<T: Send> Sync for Receiver<T> {}
unsafe impl<T: Send> Send for Receiver<T> {}

#[derive(Debug)]
struct Node<T: Send + 'static> {
    item: T,
    next: AtomicPtr<Node<T>>,
}

impl<T: Send + 'static> Deref for Node<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.item
    }
}

impl<T: Send + 'static> Deref for Node<T> {
    fn is_sentinel(&self) -> bool {
        todo!()
    }
}
