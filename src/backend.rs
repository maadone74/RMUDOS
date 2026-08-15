// backend.rs

use std::sync::Mutex;

pub struct Heartbeat {
    // For now, we'll just use a simple vector of connection addresses
    // In a real implementation, this would be a more complex data structure
    // that stores information about each object's heartbeat.
    pub list: Mutex<Vec<std::net::SocketAddr>>,
}

impl Heartbeat {
    pub fn new() -> Self {
        Heartbeat {
            list: Mutex::new(Vec::new()),
        }
    }

    pub fn set_heartbeat(&self, addr: std::net::SocketAddr, to: bool) {
        let mut list = self.list.lock().unwrap();
        if to {
            if !list.contains(&addr) {
                list.push(addr);
            }
        } else {
            list.retain(|&x| x != addr);
        }
    }

    pub fn call_heartbeat(&self) {
        let list = self.list.lock().unwrap();
        for addr in list.iter() {
            println!("Calling heartbeat for {}", addr);
        }
    }
}

pub struct Swap;

impl Swap {
    pub fn new() -> Self {
        Swap
    }

    pub fn look_for_objects_to_swap(&self) {
        println!("Looking for objects to swap...");
    }
}

