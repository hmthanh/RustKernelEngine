use crossbeam::queue::ArrayQueue;
use std::sync::Arc;
use tracing::debug;

/// Lock-free MPMC queue for task distribution
pub struct LockFreeQueue<T> {
    queue: Arc<ArrayQueue<T>>,
}

impl<T> LockFreeQueue<T> {
    /// Create a new lock-free queue with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    /// Push an item to the queue (non-blocking)
    pub fn push(&self, item: T) -> Result<(), T> {
        self.queue.push(item)
    }

    /// Pop an item from the queue (non-blocking)
    pub fn pop(&self) -> Option<T> {
        self.queue.pop()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Check if queue is full
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    /// Get current queue length
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Get queue capacity
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Clone the queue reference (shares the same underlying queue)
    pub fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}

/// Task type for the worker queue
#[derive(Debug)]
pub enum Task {
    /// Handle a new connection
    NewConnection(i32),
    /// Process HTTP request
    ProcessRequest { fd: i32, data: Vec<u8> },
    /// Send response
    SendResponse { fd: i32, response: Vec<u8> },
    /// Close connection
    CloseConnection(i32),
    /// Shutdown worker
    Shutdown,
}

/// Work-stealing queue for load balancing
pub struct WorkStealingQueue {
    local_queue: LockFreeQueue<Task>,
    shared_queues: Vec<LockFreeQueue<Task>>,
}

impl WorkStealingQueue {
    /// Create a new work-stealing queue
    pub fn new(num_workers: usize, capacity: usize) -> Vec<Self> {
        let mut queues = Vec::new();
        let mut all_queues = Vec::new();

        // Create all local queues first
        for _ in 0..num_workers {
            all_queues.push(LockFreeQueue::new(capacity));
        }

        // Build work-stealing queues with references to other queues
        for i in 0..num_workers {
            let local_queue = all_queues[i].clone();
            let mut shared_queues = Vec::new();

            for (j, queue) in all_queues.iter().enumerate() {
                if i != j {
                    shared_queues.push(queue.clone());
                }
            }

            queues.push(WorkStealingQueue {
                local_queue,
                shared_queues,
            });
        }

        queues
    }

    /// Push a task to the local queue
    pub fn push(&self, task: Task) -> Result<(), Task> {
        self.local_queue.push(task)
    }

    /// Pop a task from local queue, or steal from other queues
    pub fn pop(&self) -> Option<Task> {
        // Try local queue first
        if let Some(task) = self.local_queue.pop() {
            debug!("Got task from local queue");
            return Some(task);
        }

        // Try to steal from other queues
        for queue in &self.shared_queues {
            if let Some(task) = queue.pop() {
                debug!("Stole task from another queue");
                return Some(task);
            }
        }

        None
    }

    /// Get local queue reference for direct submission
    pub fn local(&self) -> &LockFreeQueue<Task> {
        &self.local_queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_free_queue() {
        let queue = LockFreeQueue::new(10);
        
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        
        queue.push(Task::Shutdown).unwrap();
        assert_eq!(queue.len(), 1);
        
        let item = queue.pop();
        assert!(matches!(item, Some(Task::Shutdown)));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_work_stealing() {
        let queues = WorkStealingQueue::new(3, 10);
        
        // Push to first queue
        queues[0].push(Task::Shutdown).unwrap();
        
        // Second queue should be able to steal
        let stolen = queues[1].pop();
        assert!(matches!(stolen, Some(Task::Shutdown)));
    }
}
