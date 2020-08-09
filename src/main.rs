#![allow(unused)]

use argh::FromArgs;
use async_std::{fs::File, io::ReadExt};

use color_eyre::eyre;
use sha3::Digest;
use std::path::{Path, PathBuf};

/// Prints the SHA3-256 hash of some files
#[derive(FromArgs)]
struct Args {
    /// the files whose contents to hash and print
    #[argh(positional)]
    files: Vec<PathBuf>,
}

#[async_std::main]
async fn main() -> Result<(), eyre::Error> {
    color_eyre::install().unwrap();
    let args: Args = argh::from_env();

    let mut handles = Vec::new();

    for file in &args.files {
        let file = file.clone();
        let handle = async_std::task::spawn_local(async move {
            let res = hash_file(&file).await;
            if let Err(e) = res {
                println!("While hashing {}: {}", file.display(), e);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await;
    }

    Ok(())
}

async fn hash_file(path: &Path) -> Result<(), eyre::Error> {
    let file = File::open(path).await?;
    let mut file = TracingReader { inner: file };

    let mut hasher = sha3::Sha3_256::new();

    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = Pin::new(&mut file).simple_read(&mut buf[..]).await?;
        match n {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }

    let hash = hasher.finalize();
    print!("{} ", path.display());
    for x in hash {
        print!("{:02x}", x);
    }
    println!();

    Ok(())
}

use futures::io::AsyncRead;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;

// Generate projection types
#[pin_project]
struct TracingReader<R>
where
    R: AsyncRead,
{
    // pinning is structural for `inner`
    #[pin]
    inner: R,
}

impl<R> AsyncRead for TracingReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        // tracing
        let address = &self as *const _;
        println!("{:?} => {:?}", address, std::thread::current().id());

        // reading
        self.project().inner.poll_read(cx, buf)
    }
}

use async_trait::async_trait;

#[async_trait]
trait SimpleRead {
    async fn simple_read(self: Pin<&mut Self>, buf: &mut [u8]) -> io::Result<usize>;
}

#[async_trait]
impl<R> SimpleRead for TracingReader<R>
where
    R: AsyncRead + Send,
{
    async fn simple_read(self: Pin<&mut Self>, buf: &mut [u8]) -> io::Result<usize> {
        // tracing
        let address = &self as *const _;
        println!("{:?} => {:?}", address, std::thread::current().id());

        // reading
        self.project().inner.read(buf).await
    }
}
