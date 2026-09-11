use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RingBuffer {
    capacity_chars: usize,
    buffer: VecDeque<String>,
    current_size: usize,
}

impl RingBuffer {
    pub fn new(capacity_chars: usize) -> Self {
        Self {
            capacity_chars,
            buffer: VecDeque::new(),
            current_size: 0,
        }
    }

    pub fn push(&mut self, text: String) {
        self.current_size += text.len();
        self.buffer.push_back(text);

        while self.current_size > self.capacity_chars && !self.buffer.is_empty() {
            if let Some(old) = self.buffer.pop_front() {
                self.current_size = self.current_size.saturating_sub(old.len());
            }
        }
    }

    pub fn get_backlog(&self) -> String {
        let mut result = String::with_capacity(self.current_size);
        for chunk in &self.buffer {
            result.push_str(chunk);
        }
        result
    }
}
