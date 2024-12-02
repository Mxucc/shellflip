//! 示例重启应用程序。
//! 该程序实现了一个TCP服务器，接受连接，
//! 输出一行简短的信息描述正在运行的进程，
//! 然后回显客户端发送给它的任何信息。
//!
//! 当应用程序运行时，可以通过`restart`命令调用另一个实例来触发重启。现有的连接将被保持，而旧的进程将在所有客户端断开连接后终止。新进程将在另一个套接字上监听（因为这个库不提供套接字继承或重新绑定的功能）。

use anyhow::Error;
use async_trait::async_trait;
use clap::{Parser, Subcommand};
use shellflip::lifecycle::*;
use shellflip::{RestartConfig, ShutdownCoordinator, ShutdownHandle, ShutdownSignal};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::{pin, select};

/// 用于测试优雅关闭和重启的简单程序
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
    /// 重启协调套接字路径
    #[arg(short, long, default_value = "/tmp/restarter.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// 触发重启
    Restart,
}

struct AppData {
    restart_generation: u32,
}

#[async_trait]
impl LifecycleHandler for AppData {
    async fn send_to_new_process(&mut self, mut write_pipe: PipeWriter) -> std::io::Result<()> {
        if self.restart_generation > 4 {
            log::info!("四次重启已经足够多了，对吧？");
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "操作成功完成",
            ));
        }
        write_pipe.write_u32(self.restart_generation).await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    let args = Args::parse();
    let mut app_data = AppData {
        restart_generation: 0,
    };

    if let Some(mut handover_pipe) = receive_from_old_process() {
        app_data.restart_generation = handover_pipe.read_u32().await? + 1;
    }

    let restart_generation = app_data.restart_generation;

    // 配置实现优雅重启所需的基本要求。
    let restart_conf = RestartConfig {
        enabled: true,
        coordination_socket_path: args.socket.into(),
        lifecycle_handler: Box::new(app_data),
        ..Default::default()
    };

    match args.command {
        // 重启一个已经在运行的进程
        Some(Commands::Restart) => {
            let res = restart_conf.request_restart().await;
            match res {
                Ok(id) => {
                    log::info!("重启成功，子进程ID是{}", id);
                    return Ok(());
                }
                Err(e) => {
                    log::error!("重启失败: {}", e);
                    return Err(e);
                }
            }
        }
        // 标准操作模式
        None => {}
    }

    // 启动重启线程并获取一个任务，当重启完成时该任务会完成。
    let restart_task = restart_conf.try_into_restart_task()?;
    // （由于下面的循环需要使用pin!）
    pin!(restart_task);
    // 创建一个关闭协调器，以便我们可以等待所有客户端连接完成。
    let shutdown_coordinator = ShutdownCoordinator::new();
    // 绑定一个TCP监听套接字，给我们一些事情做
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!(
        "实例号{} 正在监听 {}",
        restart_generation,
        listener.local_addr().unwrap()
    );

    loop {
        select! {
            res = listener.accept() => {
                match res {
                    Ok((sock, addr)) => {
                        log::info!("收到来自{}的连接", addr);
                        // 分配一个新的任务来处理客户端连接。
                        // 给它一个关闭句柄，这样我们就可以等待其完成。
                        tokio::spawn(echo(sock, shutdown_coordinator.handle()));
                    }
                    Err(e) => {
                        log::warn!("接受错误: {}", e);
                    }
                }
            }
            res = &mut restart_task => {
                match res {
                    Ok(_) => {
                        log::info!("重启成功，等待任务完成");
                    }
                    Err(e) => {
                        log::error!("重启任务失败: {}", e);
                    }
                }
                // 等待所有客户端完成。
                shutdown_coordinator.shutdown().await;
                log::info!("退出...");
                return Ok(());
            }
        }
    }
}

async fn echo(mut sock: TcpStream, shutdown_handle: Arc<ShutdownHandle>) {
    // 获取关闭请求的通知。
    // 注意，在此任务的整个生命周期中，我们仍然保持shutdown_handle处于活动状态。
    let mut shutdown_signal = ShutdownSignal::from(&*shutdown_handle);
    let mut buf = [0u8; 1024];
    let out = format!("你好，这是进程{}\n", std::process::id());
    let _ = sock.write_all(out.as_bytes()).await;

    loop {
        select! {
            r = sock.read(&mut buf) => {
                match r {
                    Ok(0) => return,
                    Ok(n) => {
                        if let Err(e) = sock.write_all(&buf[..n]).await {
                            log::error!("写入失败: {}", e);
                            return;
                        }
                    }
                    Err(e) => {
                        log::error!("读取失败: {}", e);
                        return;
                    }
                }
            }
            _ = shutdown_signal.on_shutdown() => {
                log::info!("已请求关闭，但客户端{}仍然活跃", sock.peer_addr().unwrap());
            }
        }
    }
}