#![allow(unused)]

use argh::FromArgs;
use async_std::{fs::File, io::ReadExt};

use color_eyre::eyre;
use sha3::Digest;
use std::path::{Path, PathBuf};

use pin_project::pin_project;
use tracing_subscriber::{prelude::*, Registry};

/// Prints the SHA3-256 hash of some files
#[derive(FromArgs)]
struct Args {
    /// the files whose contents to hash and print
    #[argh(positional)]
    files: Vec<PathBuf>,
}

#[async_std::main]
#[tracing::instrument]
async fn main() -> Result<(), eyre::Error> {
    // let subscriber = Registry::default().with(HierarchicalLayer::new(2));
    // tracing::subscriber::set_global_default(subscriber).unwrap();

    color_eyre::install().unwrap();
    let args: Args = argh::from_env();

    let mut handles = Vec::new();

    for file in &args.files {
        let file = file.clone();
        let handle = async_std::task::spawn(async move {
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
    let file = TracingReader { inner: file };
    let mut file = SimpleAsyncReader {
        state: State::Idle(file, Default::default()),
    };

    let mut hasher = sha3::Sha3_256::new();

    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf[..]).await?;
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

use futures::{io::AsyncRead, Future};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

// Generate projection types
struct TracingReader<R>
where
    R: AsyncRead,
{
    inner: R,
}

use async_trait::async_trait;
use tracing_tree::HierarchicalLayer;

#[async_trait]
trait SimpleRead {
    async fn simple_read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

#[async_trait]
impl<R> SimpleRead for TracingReader<R>
where
    R: AsyncRead + Send + Unpin,
{
    #[tracing::instrument(skip(self, buf))]
    async fn simple_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use futures_timer::Delay;
        use std::time::Duration;

        // artificial slowdown
        tracing::debug!("doing delay...");
        Delay::new(Duration::from_millis(50)).await;
        tracing::debug!("doing delay...done!");

        // reading
        tracing::debug!("doing read...");
        let res = self.inner.read(buf).await;
        tracing::debug!("doing read...done!");
        res
    }
}

#[pin_project]
struct SimpleAsyncReader<R>
where
    R: SimpleRead,
{
    state: State<R>,
}

type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

enum State<R> {
    Idle(R, Vec<u8>),
    Pending(BoxFut<(R, Vec<u8>, io::Result<usize>)>),
    Transitional,
}

impl<R> AsyncRead for SimpleAsyncReader<R>
where
    R: SimpleRead + Send + 'static,
{
    #[tracing::instrument(skip(self, buf))]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let proj = self.project();
        let mut state = State::Transitional;
        std::mem::swap(proj.state, &mut state);

        let mut fut = match state {
            State::Idle(mut inner, mut internal_buf) => {
                tracing::debug!("getting new future...");
                internal_buf.clear();
                internal_buf.reserve(buf.len());
                unsafe { internal_buf.set_len(buf.len()) }

                Box::pin(async move {
                    let res = inner.simple_read(&mut internal_buf[..]).await;
                    (inner, internal_buf, res)
                })
            }
            State::Pending(fut) => {
                tracing::debug!("polling existing future...");
                fut
            }
            State::Transitional => unreachable!(),
        };

        match fut.as_mut().poll(cx) {
            Poll::Ready((inner, mut internal_buf, result)) => {
                tracing::debug!("future was ready!");
                if let Ok(n) = &result {
                    let n = *n;
                    unsafe { internal_buf.set_len(n) }

                    let dst = &mut buf[..n];
                    let src = &internal_buf[..];
                    dst.copy_from_slice(src);
                } else {
                    unsafe { internal_buf.set_len(0) }
                }
                *proj.state = State::Idle(inner, internal_buf);
                Poll::Ready(result)
            }
            Poll::Pending => {
                tracing::debug!("future was pending!");
                *proj.state = State::Pending(fut);
                Poll::Pending
            }
        }
    }
}
