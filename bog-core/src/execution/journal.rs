use crate::execution::{Fill, Order, OrderId};
use anyhow::Result;
use crossbeam::channel::{bounded, Sender, Receiver};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};
use tracing::{error, info};

/// Journal event for persistence and recovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data")]
pub enum JournalEvent {
    OrderSubmit(Order),
    OrderAck(OrderId),
    Fill(Fill),
    OrderCancel(OrderId),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: u64,
    #[serde(flatten)]
    pub event: JournalEvent,
}

impl JournalEntry {
    pub fn new(event: JournalEvent) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_millis() as u64;

        Self { timestamp, event }
    }
}

pub struct AsyncJournal {
    sender: Option<Sender<JournalEvent>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl AsyncJournal {
    pub fn new(path: PathBuf) -> Result<Self> {
        // Buffer size of 4096 events should be sufficient for bursts
        let (sender, receiver) = bounded(4096);

        let handle = thread::spawn(move || {
            Self::writer_loop(path, receiver);
        });

        Ok(Self {
            sender: Some(sender),
            thread_handle: Some(handle),
        })
    }

    fn writer_loop(path: PathBuf, receiver: Receiver<JournalEvent>) {
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open journal file {:?}: {}", path, e);
                return;
            }
        };

        // Process events
        for event in receiver {
            let entry = JournalEntry::new(event);
            match serde_json::to_string(&entry) {
                Ok(json) => {
                    if let Err(e) = writeln!(file, "{}", json) {
                        error!("Failed to write to journal: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to serialize journal entry: {}", e);
                }
            }
        }
        
        // Receiver disconnected, flush and exit
        if let Err(e) = file.flush() {
            error!("Failed to flush journal: {}", e);
        }
        info!("AsyncJournal writer thread stopping");
    }

    pub fn record(&self, event: JournalEvent) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.try_send(event) {
                 // If buffer is full, we log error but don't block. 
                 // In production HFT, dropping log events is preferable to stalling the engine.
                 error!("AsyncJournal buffer full or disconnected, dropping event: {:?}", e);
            }
        }
    }
}

impl Drop for AsyncJournal {
    fn drop(&mut self) {
        // Drop sender to signal thread to stop.
        // We must do this explicitly before joining, otherwise the thread will block on receiver.recv() forever.
        let _ = self.sender.take();
        
        // We wait for thread to finish if we have a handle.
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use tempfile::NamedTempFile;
    use rust_decimal_macros::dec;
    use crate::execution::{Side, OrderType, TimeInForce, OrderStatus};
    use rust_decimal::Decimal;

    // Helper to create a dummy order
    fn create_dummy_order() -> Order {
        let now = SystemTime::now();
        Order {
            id: crate::execution::OrderId::new("test-id".to_string()),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: dec!(10000),
            size: dec!(1),
            time_in_force: TimeInForce::GTC,
            status: OrderStatus::Pending,
            filled_size: Decimal::ZERO,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_async_journal_writes() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        
        // Create journal in a scope so it drops and flushes
        {
            let journal = AsyncJournal::new(path.clone()).unwrap();
            let order = create_dummy_order();
            let event = JournalEvent::OrderSubmit(order.clone());
            
            // Record event
            journal.record(event);
        }
        
        // Read back
        let file = File::open(&path).unwrap();
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<_, _>>().unwrap();
        
        assert_eq!(lines.len(), 1);
        let entry: JournalEntry = serde_json::from_str(&lines[0]).unwrap();
        
        match entry.event {
            JournalEvent::OrderSubmit(o) => assert_eq!(o.id.as_str(), "test-id"),
            _ => panic!("Wrong event type"),
        }
    }
}

