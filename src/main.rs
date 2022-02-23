use std::fs::File;
use std::io::Error;
use std::io::{Read, Write};
use std::net::Ipv4Addr;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::time;

use serde::{Deserialize, Serialize};
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::hash::Hasher;
use xxhash_rust::xxh3;
//use serde_json::Result as SerdeResult;

const PORT: u16 = 7654;
const SAVE: &str = ".idlecoin";

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CoinsGen {
    name: u64, // hash of name / wallet address
    coin: u64, // total idlecoin
    iter: u64, // session iteration idlecoin
    gen: u64,  // generated idlecoin
}

fn main() {
    // Create global array of user generators
    let generators = Arc::new(Mutex::new(Vec::<CoinsGen>::new()));

    load_stats(&generators);
    // Bind network listener to port
    let listener = match TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT))) {
        Ok(l) => l,
        Err(_) => {
            println!(
                "Cannot bind to port: {}. Is idlecoin already running?",
                PORT
            );
            return;
        }
    };

    let mut signals = Signals::new(&[SIGINT]).unwrap();
    let generators_save = Arc::clone(&generators);
    thread::spawn(move || {
        for sig in signals.forever() {
            if sig == SIGINT {
                save_stats(generators_save);
                std::process::exit(0);
            }
        }
    });

    // Listen for connections
    for stream in listener.incoming() {
        let s = match stream {
            Ok(s) => s,
            _ => continue,
        };

        // Handle connection in new thread
        let generators_close = Arc::clone(&generators);
        thread::spawn(move || {
            match session(s, generators_close) {
                Ok(_) => (),
                Err(s) => println!("Err: {}", s),
            };
        });
    }
}

fn login(
    mut stream: &TcpStream,
    generators: &Arc<Mutex<Vec<CoinsGen>>>,
) -> Result<CoinsGen, Error> {
    // Lock generators
    let gens = generators.lock().unwrap();

    // Request username
    let msg = format!(
        "Welcome to Idlecoin! There are {} current users.\nPlease enter your account: ",
        gens.len()
    );
    stream.write_all(msg.as_bytes())?;

    // Read username
    let mut name_raw: [u8; 1024] = [0; 1024];
    let _ = stream.read(&mut name_raw[..]).unwrap();

    let mut hash = xxh3::Xxh3::new();
    hash.write(&name_raw);
    let name = hash.finish();

    // Look for user record
    for i in gens.deref() {
        if name == i.name {
            return Ok(*i);
        }
    }

    // Create new record
    Ok(CoinsGen {
        name,
        coin: 0,
        iter: 0,
        gen: 0,
    })
    // Unlock generators
}

fn update_generator(
    generators: &Arc<Mutex<Vec<CoinsGen>>>,
    mut coin: &mut CoinsGen,
) -> Result<(), Error> {
    let mut gens = generators.lock().unwrap();
    for i in gens.deref() {
        if i.name == coin.name {
            coin.coin = i.coin + (coin.gen - coin.iter);
            coin.iter = coin.gen;
        }
    }
    gens.retain(|x| x.name != coin.name);
    gens.push(*coin);
    drop(gens);

    Ok(())
}

fn session(mut stream: TcpStream, generators: Arc<Mutex<Vec<CoinsGen>>>) -> Result<(), Error> {
    // Allow user session to login
    let mut miner = login(&stream, &generators)?;
    //let initcoin = gen.coin;
    miner.gen = 1;
    miner.iter = 0;

    let mut inc = 1;
    let mut level = 1;
    let mut pow = 10;

    // Main loop
    loop {
        // Level up
        if miner.gen > pow {
            level += 1;
            inc = 1 << level;
            pow *= 10;
            update_generator(&generators, &mut miner)?;
            let msg = format!(
                "\n===\nIdlecoin generator upgrade to level: {}\nIDLECOIN wallet: {}\nIDLECOIN generated: {}\n===\n",
                level, miner.coin, miner.gen
            );
            match stream.write_all(msg.as_bytes()) {
                Ok(_) => (),
                Err(_) => break,
            };
        }

        // Increment coins
        let msg = format!("\rGenerating idlecoins: {}", miner.gen);
        match stream.write_all(msg.as_bytes()) {
            Ok(_) => (),
            Err(_) => break,
        };
        miner.gen += inc;

        // Rest from all that work
        sleep(time::Duration::from_millis(100));
    }

    update_generator(&generators, &mut miner)?;

    Ok(())
}

fn load_stats(generators: &Arc<Mutex<Vec<CoinsGen>>>) {
    let mut j = String::new();

    let mut file = File::open(&SAVE).unwrap();
    file.read_to_string(&mut j).unwrap();

    if j.is_empty() {
        return;
    }
    println!("Loading stats...");

    let mut c: Vec<CoinsGen> = serde_json::from_str(&j).unwrap();
    if c.is_empty() {
        println!("Failed to load {}", SAVE);
        return;
    }

    let mut gens = generators.lock().unwrap();
    gens.append(&mut c);
    drop(gens);

    println!("Successfully loaded stats file {}", SAVE);
}

fn save_stats(generators: Arc<Mutex<Vec<CoinsGen>>>) {
    println!("Saving stats...");
    let gens = generators.lock().unwrap();
    let j = serde_json::to_string(&gens.deref()).unwrap();

    let mut file: File;
    file = match File::create(&SAVE) {
        Ok(f) => f,
        Err(_) => {
            println!("Error opening {} for writing!", SAVE);
            return;
        }
    };

    let len = file.write(j.as_bytes()).unwrap();
    if j.len() != len {
        println!("Error writing save data to {}", SAVE);
        return;
    }

    println!("Successfully saved data to {}", SAVE);
}
