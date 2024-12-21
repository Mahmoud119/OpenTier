use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc, 
        Arc, 
        Mutex, 
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

// Define the number of threads in the thread pool
const THREAD_POOL_SIZE: usize = 4;

// ThreadPool struct to manage workers and task dispatching
struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>, // Sender for sending jobs to workers
}


type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    // Create a new thread pool with a given size
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "Thread pool size must be greater than zero");

        let (sender, receiver) = mpsc::channel(); // Create a channel for job dispatching
        let receiver = Arc::new(Mutex::new(receiver)); // Wrap receiver in Arc and Mutex for shared access

        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver))); // Create workers
        }

        ThreadPool {
            workers,
            sender: Some(sender), // Store the sender for job submission
        }
    }

    // Submit a job to the thread pool
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(sender) = &self.sender {
            sender.send(Box::new(f)).unwrap(); // Send the job to the worker queue
        }
    }
}

impl Drop for ThreadPool {
    // Drop implementation ensures all threads finish before the ThreadPool is destroyed
    fn drop(&mut self) {
        drop(self.sender.take()); // Close the sender to signal workers to stop

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap(); // Join worker threads to ensure clean shutdown
            }
        }
    }
}

// Worker struct for executing jobs
struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>, // Handle to the worker's thread
}

impl Worker {
    // Create a new worker with an ID and shared job receiver
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let thread = thread::spawn(move || loop {
            // Lock the receiver and wait for a job
            let job = receiver.lock().unwrap().recv();
            match job {
                Ok(job) => {
                    println!("Worker {id} got a job; executing."); // Log job execution
                    job(); // Execute the job
                }
                Err(_) => {
                    println!("Worker {id} disconnected; shutting down."); // Log shutdown
                    break;
                }
            }
        });

        Worker {
            id,
            thread: Some(thread), // Store the worker's thread handle
        }
    }
}

// Handle incoming HTTP connection
fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap(); // Read data from the connection

    let response = "HTTP/1.1 200 OK\r\n\r\nHello, world!"; // Simple HTTP response
    stream.write(response.as_bytes()).unwrap(); // Write the response to the stream
    stream.flush().unwrap(); // Ensure all data is sent
}

// Server struct for the TCP listener and thread pool
pub struct Server {
    listener: TcpListener,
    thread_pool: ThreadPool,
    is_running: Arc<AtomicBool>, // Atomic flag to indicate server running state
}

impl Server {
    // Stop the server by setting the is_running flag to false
    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Server {
    // Create a new server and bind to the given address
    pub fn new(addr: &str) -> Self {
        let listener = TcpListener::bind(addr).unwrap(); // Bind to the address
        let thread_pool = ThreadPool::new(THREAD_POOL_SIZE); // Create the thread pool
        Server {
            listener,
            thread_pool,
            is_running: Arc::new(AtomicBool::new(true)), // Initialize is_running
        }
    }

    // Start the server and handle incoming connections
    pub fn run(&self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    // Submit connection handling to the thread pool
                    self.thread_pool.execute(|| {
                        handle_connection(stream);
                    });
                }
                Err(e) => {
                    println!("Connection failed: {}", e); // Log connection error
                }
            }
        }
    }
}
