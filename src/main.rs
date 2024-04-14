use std::io::{BufRead, Write, BufReader, BufWriter};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::str::from_utf8;
use std::thread;
use std::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::sync::Arc;

static CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

const ADDRESS: &str = "127.0.0.1:7878";

fn handle_client(stream: TcpStream, client_id: usize) {
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);
    let mut buffer = Vec::new();

    loop {
        match reader.read_until(b'\n', &mut buffer) {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break;
                }
                match from_utf8(&buffer) {
                    Ok(input_str) => {
                        match input_str.trim().parse::<f64>() {
                            Ok(x) => {
                                let r = 4.0;
                                let x_next = r * x * (1.0 - x);
                                let response = format!("{:.12}\n", x_next);
                                if let Err(e) = writer.write_all(response.as_bytes()) {
                                    eprintln!("Client {} failed to write to client: {}", client_id, e);
                                    break;
                                }
                                if let Err(e) = writer.flush() {
                                    eprintln!("Client {} failed to flush writer: {}", client_id, e);
                                    break;
                                }
                            },
                            Err(e) => eprintln!("Client {} error parsing number from client: {}", client_id, e),
                        }
                    },
                    Err(e) => eprintln!("Client {} error decoding UTF-8 from client: {}", client_id, e),
                }
                buffer.clear();
            },
            Err(e) => {
                eprintln!("Client {} error reading from client: {}", client_id, e);
                break;
            },
        }
    }

    if let Err(e) = stream.shutdown(Shutdown::Both) {
        eprintln!("Client {} failed to shutdown stream: {}", client_id, e);
    }
}

fn spawn_server(continue_running: Arc<AtomicBool>) {
    let listener = TcpListener::bind(ADDRESS).unwrap_or_else(|err| {
        panic!("Failed to bind to {}: {}", ADDRESS, err);
    });
    listener.set_nonblocking(true).expect("Failed to set non-blocking");

    println!("Server listening on {}", ADDRESS);

    while continue_running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let client_id = CLIENT_ID.fetch_add(1, Ordering::SeqCst);
                thread::spawn(move || {
                    handle_client(stream, client_id);
                });
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            },
            Err(_) => {
                if continue_running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }
}

fn client_process(initial_value: f64, client_id: usize) {
    let stream = TcpStream::connect(ADDRESS).unwrap_or_else(|err| {
        panic!("Failed to connect to server: {}", err);
    });
    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    let mut current_value = initial_value;
    for msg in 0..50 {
        let message = format!("{:.12}\n", current_value);
        if let Err(e) = writer.write_all(message.as_bytes()) {
            eprintln!("Client {} failed to send data: {}, message number: {}", client_id, e, msg + 1);
            break;
        }
        if let Err(e) = writer.flush() {
            eprintln!("Client {} failed to flush writer: {}, message number: {}", client_id, e, msg);
            break;
        }

        let mut buffer = Vec::new();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(bytes) => {
                if bytes == 0 {
                    eprintln!("Client {} no data received, terminating.", client_id);
                    break;
                }
                if let Ok(response) = from_utf8(&buffer) {
                    current_value = response.trim().parse().unwrap_or_else(|err| {
                        panic!("Client {} failed to parse response: {}, message number: {}", client_id, err, msg + 1);
                    });
                    println!("Client {} received: {:.12}, message number: {}", client_id, current_value, msg + 1);

                    let delay_ms = (current_value * 1000.0) as u64 % 5000;
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            },
            Err(e) => {
                eprintln!("Client {} failed to read data: {}, message number: {}", client_id, e, msg + 1);
                break;
            },
        }
    }
    if let Err(e) = stream.shutdown(Shutdown::Both) {
        eprintln!("Client {} failed to shutdown the connection: {}", client_id, e);
    }
}

fn main() -> std::io::Result<()> {
    let continue_running = Arc::new(AtomicBool::new(true));
    let server_continue = continue_running.clone();

    let server_thread = thread::spawn(move || {
        spawn_server(server_continue);
    });

    let clients: Vec<_> = (0..5).map(|i| {
        let client_id = CLIENT_ID.fetch_add(1, Ordering::SeqCst);
        let initial_value = 0.2 + i as f64 * 0.02; // Different initial conditions for clients
        thread::spawn(move || {
            client_process(initial_value, client_id);
        })
    }).collect();

    for client in clients {
        let _ = client.join();
    }

    continue_running.store(false, Ordering::SeqCst);
    server_thread.join().unwrap();

    Ok(())
}
