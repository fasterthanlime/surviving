use argh::FromArgs;
use color_eyre::eyre;
use sha3::Digest;
use std::{fs::File, io::Read, path::PathBuf};

/// Prints the SHA3-256 hash of a file.
#[derive(FromArgs)]
struct Args {
    /// the file whose contents to hash and print
    #[argh(positional)]
    file: PathBuf,
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install().unwrap();
    let args: Args = argh::from_env();

    let mut file = File::open(&args.file)?;
    let mut hasher = sha3::Sha3_256::new();

    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf[..])?;
        match n {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }

    let hash = hasher.finalize();
    print!("{} ", args.file.display());
    for x in hash {
        print!("{:02x}", x);
    }
    println!();

    Ok(())
}
