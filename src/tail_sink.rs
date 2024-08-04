use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, MutexGuard};

use futuresdr::anyhow::{Ok, Result};
use futuresdr::runtime::MessageIo;
use futuresdr::{
    macros::async_trait,
    runtime::{
        Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIoBuilder, StreamIo, StreamIoBuilder,
        WorkIo,
    },
};

//There should be one thread reading and one writing
pub struct TailRing<T>
where
    T: Send + 'static + Copy + std::fmt::Display + Default,
{
    //tail = 0 none is current
    //tail = 1 a is current
    //tail = 2 b is current
    //tail = 3 c is current
    tail_state: AtomicUsize,
    a: Mutex<Vec<T>>,
    b: Mutex<Vec<T>>,
    c: Mutex<Vec<T>>,
    buffer_size: usize,
}

impl<T: Send + 'static + Copy + std::fmt::Display + Default> TailRing<T> {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            tail_state: AtomicUsize::new(0),
            a: vec![T::default(); buffer_size].into(),
            b: vec![T::default(); buffer_size].into(),
            c: vec![T::default(); buffer_size].into(),
            buffer_size,
        }
    }

    pub fn get_lease(&self) -> (MutexGuard<Vec<T>>, usize) {
        let tail_state = self.tail_state.load(std::sync::atomic::Ordering::Relaxed);
        if tail_state == 0 {
            return (self.a.lock().unwrap(), 1);
        } else if tail_state == 2 {
            if let core::result::Result::Ok(guard) = self.c.try_lock() {
                return (guard, 3);
            } else {
                return (self.a.lock().unwrap(), 1);
            }
        } else if tail_state == 1 {
            if let core::result::Result::Ok(guard) = self.b.try_lock() {
                return (guard, 2);
            } else {
                return (self.c.lock().unwrap(), 3);
            }
        } else if let core::result::Result::Ok(guard) = self.a.try_lock() {
            return (guard, 1);
        } else {
            return (self.b.lock().unwrap(), 2);
        };
    }

    pub fn get(&self) -> Result<MutexGuard<Vec<T>>, ()> {
        let tail_state = self.tail_state.load(std::sync::atomic::Ordering::Relaxed);
        if tail_state == 1 {
            let guard = self.a.try_lock();
            if let core::result::Result::Ok(buff) = guard {
                core::result::Result::Ok(buff)
            } else {
                Err(())
            }
        } else if tail_state == 2 {
            let guard = self.b.try_lock();
            if let core::result::Result::Ok(buff) = guard {
                core::result::Result::Ok(buff)
            } else {
                Err(())
            }
        } else if tail_state == 3 {
            let guard = self.c.try_lock();
            if let core::result::Result::Ok(buff) = guard {
                core::result::Result::Ok(buff)
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }
}

pub struct TailSink<T>
where
    T: Send + 'static + Copy + std::fmt::Display + Default,
{
    ring: Arc<TailRing<T>>,
    filled: usize,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static + Copy + std::fmt::Display + Default> TailSink<T> {
    /// Create Tail Sink block
    #[allow(clippy::new_ret_no_self)]
    pub fn new(ring: Arc<TailRing<T>>) -> Block {
        Block::new(
            BlockMetaBuilder::new("TailSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            TailSink::<T> {
                ring,
                filled: 0,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static + Copy + std::fmt::Display + Default> Kernel for TailSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let mut items = i.len();
        let total_items = items;
        let mut offset = 0;
        if items > 0 {
            while items != 0 {
                //items > buffer - filled
                //items == buffer - filled
                //items < buffer - filled
                let mut lease = self.ring.get_lease();
                let to_fill: usize = if self.filled == self.ring.buffer_size {
                    self.ring
                        .tail_state
                        .store(lease.1, std::sync::atomic::Ordering::Relaxed);
                    drop(lease); // This is needed to unlock the mutex
                    lease = self.ring.get_lease();

                    self.filled = 0;
                    self.ring.buffer_size
                } else {
                    self.ring.buffer_size - self.filled
                }
                .min(items);

                for (idx, t) in i[offset..offset + to_fill].iter().enumerate() {
                    lease.0[self.filled + idx] = *t;
                }
                offset += to_fill;
                self.filled += to_fill;
                items -= to_fill;
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(total_items);
        Ok(())
    }
}
