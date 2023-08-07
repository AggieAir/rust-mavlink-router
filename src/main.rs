use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::collections::HashMap;
use std::time::Duration;

use mavlink::{self, common, Message};

fn main() {
    let urls = std::env::args().skip(1).collect::<Vec<_>>();

    if urls.len() == 0 {
        eprintln!("No arguments, exiting now");
        std::process::exit(1);
    }

    let mut raw_connections = Vec::with_capacity(urls.len());

    let map: Arc<Mutex<HashMap<(u8, u8), u8>>> = Arc::new(Mutex::new(HashMap::new()));

    for (idx, url) in urls.iter().enumerate() {
        println!("opening connection to {} (#{})", url, idx);
        let mut connection = mavlink::connect::<common::MavMessage>(url).unwrap();
        connection.set_protocol_version(mavlink::MavlinkVersion::V2);
        raw_connections.push((idx, Arc::new(connection)));
    }

    let connections = Arc::new(raw_connections);

    let join_handles = connections
        .iter()
        .map(|(idx, connection)| {
            let my_connections = Arc::clone(&connections);
            let my_connection = Arc::clone(&connection);
            let my_idx = idx.clone();
            let my_map = map.clone();

            thread::spawn(move || {
                println!("starting thread {}", my_idx);
                loop {
                    let (header, message) = my_connection.recv().unwrap();

                    let id = (header.system_id, header.component_id);

                    println!(
                        "endpoint #{}: message 0x{:02X} from ({:3}, {:3}) -- id={:3} {:?}",
                        my_idx,
                        header.sequence, header.system_id, header.component_id,
                        message.message_id(), message.message_name()
                    );

                    for (recv_idx, receiver) in my_connections.as_slice() {
                        if *recv_idx != my_idx {
                            receiver.send(&header, &message).unwrap();
                        }
                    }

                    my_map.lock().unwrap().entry(id)
                        .and_modify(|seq| {
                            if seq.overflowing_add(1).0 != header.sequence {
                                println!("endpoint #{}: missing {} values (prev={}, next={})", my_idx, header.sequence.overflowing_sub(*seq).0, *seq, header.sequence);
                            }
                            *seq = header.sequence;
                        })
                        .or_insert_with(|| header.sequence);
                }
            })
        })
        .collect::<Vec<_>>();

    for jh in join_handles {
        jh.join().unwrap();
    }
}
