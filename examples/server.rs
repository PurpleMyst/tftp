use std::env;

use tftp::Server;

fn main() {
    let mut args = env::args().skip(1);
    let addr = args.next().unwrap();
    let wd = args.next().unwrap();

    let server = Server::new(addr.clone(), wd).unwrap();
    println!("Serving Trivial File Transfer Protocol (TFTP) @ {}", addr);

    while let Ok(h) = server.serve() {
        print!("Handling request...");
        match h.handle() {
            Ok(()) => println!("OK"),
            Err(e) => println!("FAIL: {:?}", e),
        }
    }
}
