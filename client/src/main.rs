use rand::Rng;
use serde::{Deserialize, Serialize};
use ws::connect;

#[derive(Serialize, Deserialize, Debug)]
enum Command {
    NoOperate,

    Hello,
    Reload,

    Ping,
    Pong,

    Message { id: u64, message: String },
}

#[derive(Serialize, Deserialize, Debug)]
struct ControllerCommand {
    id: u64,
    controller_id: u32,
    from_ip_address: String,
    from_port_number: u32,
    command: Command,
}

fn main() {
    println!("Hello, world!");

    let mut rnd_gen = rand::rng();

    let controller_id = rnd_gen.random();
    let from_ip_address = "127.0.0.1";
    let from_port_number = 62007;

    let hello = ControllerCommand {
        id: rnd_gen.random(),
        controller_id,
        from_ip_address: from_ip_address.to_string(),
        from_port_number,
        command: Command::Hello,
    };

    let reload = ControllerCommand {
        id: rnd_gen.random(),
        controller_id,
        from_ip_address: from_ip_address.to_string(),
        from_port_number,
        command: Command::Reload,
    };

    connect("ws://localhost:5776/**lightrain_controller**/", |out| {
        out.send(serde_json::to_string(&hello).unwrap()).unwrap();
        out.send("Reload").unwrap();

        move |msg| {
            println!("Received: {}", msg);
            out.close(ws::CloseCode::Normal)
        }
    })
    .unwrap();
}
