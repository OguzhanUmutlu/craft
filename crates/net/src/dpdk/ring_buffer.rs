use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketDescriptor {
    pub packet_id: u64,
    pub data: Bytes,
    pub rx_timestamp_nanos: u64,
    pub length: usize,
    pub port: u16,
}

impl PacketDescriptor {
    pub fn new(packet_id: u64, data: Bytes, port: u16) -> Self {
        let length = data.len();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self {
            packet_id,
            data,
            rx_timestamp_nanos: now,
            length,
            port,
        }
    }
}

/// Cache-line aligned padded index to prevent false sharing
#[repr(align(64))]
struct CachePaddedUsize {
    value: AtomicUsize,
}

impl CachePaddedUsize {
    fn new(v: usize) -> Self {
        Self {
            value: AtomicUsize::new(v),
        }
    }
}

pub struct PacketRingBuffer {
    capacity: usize,
    mask: usize,
    buffer: Box<[std::sync::Mutex<Option<PacketDescriptor>>]>,
    head: CachePaddedUsize, // Write index
    tail: CachePaddedUsize, // Read index
    dropped: AtomicU64,
}

impl PacketRingBuffer {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(16);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(std::sync::Mutex::new(None));
        }

        Self {
            capacity: cap,
            mask: cap - 1,
            buffer: slots.into_boxed_slice(),
            head: CachePaddedUsize::new(0),
            tail: CachePaddedUsize::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        let head = self.head.value.load(Ordering::Acquire);
        let tail = self.tail.value.load(Ordering::Acquire);
        head.saturating_sub(tail)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn enqueue(&self, desc: PacketDescriptor) -> Result<(), PacketDescriptor> {
        let head = self.head.value.load(Ordering::Relaxed);
        let tail = self.tail.value.load(Ordering::Acquire);

        if head.wrapping_sub(tail) >= self.capacity {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return Err(desc);
        }

        let slot = &self.buffer[head & self.mask];
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(desc);
            self.head.value.store(head.wrapping_add(1), Ordering::Release);
            Ok(())
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            Err(desc)
        }
    }

    pub fn dequeue(&self) -> Option<PacketDescriptor> {
        let tail = self.tail.value.load(Ordering::Relaxed);
        let head = self.head.value.load(Ordering::Acquire);

        if tail == head {
            return None;
        }

        let slot = &self.buffer[tail & self.mask];
        if let Ok(mut guard) = slot.lock() {
            if let Some(desc) = guard.take() {
                self.tail.value.store(tail.wrapping_add(1), Ordering::Release);
                return Some(desc);
            }
        }
        None
    }

    pub fn enqueue_burst(&self, packets: &mut Vec<PacketDescriptor>) -> usize {
        let mut count = 0;
        while let Some(pkt) = packets.pop() {
            if self.enqueue(pkt).is_ok() {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    pub fn dequeue_burst(&self, max_batch: usize) -> Vec<PacketDescriptor> {
        let mut batch = Vec::with_capacity(max_batch);
        for _ in 0..max_batch {
            if let Some(pkt) = self.dequeue() {
                batch.push(pkt);
            } else {
                break;
            }
        }
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_fifo() {
        let ring = PacketRingBuffer::new(64);
        assert_eq!(ring.len(), 0);

        let p1 = PacketDescriptor::new(1, Bytes::from_static(b"packet_one"), 25565);
        let p2 = PacketDescriptor::new(2, Bytes::from_static(b"packet_two"), 25565);

        assert!(ring.enqueue(p1.clone()).is_ok());
        assert!(ring.enqueue(p2.clone()).is_ok());
        assert_eq!(ring.len(), 2);

        let out1 = ring.dequeue().unwrap();
        assert_eq!(out1.packet_id, 1);
        assert_eq!(out1.data, Bytes::from_static(b"packet_one"));

        let out2 = ring.dequeue().unwrap();
        assert_eq!(out2.packet_id, 2);
        assert_eq!(out2.data, Bytes::from_static(b"packet_two"));

        assert_eq!(ring.len(), 0);
        assert!(ring.dequeue().is_none());
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let ring = PacketRingBuffer::new(16);
        let cap = ring.capacity();

        for i in 0..cap {
            let pkt = PacketDescriptor::new(i as u64, Bytes::from_static(b"fill"), 25565);
            assert!(ring.enqueue(pkt).is_ok());
        }

        // Next should fail and record drop
        let overflow_pkt = PacketDescriptor::new(999, Bytes::from_static(b"overflow"), 25565);
        assert!(ring.enqueue(overflow_pkt).is_err());
        assert_eq!(ring.dropped_count(), 1);
    }
}
