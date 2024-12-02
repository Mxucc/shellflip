# shellflip

[![crates.io](https://img.shields.io/crates/v/shellflip.svg)](https://crates.io/crates/shellflip)
[![docs.rs](https://docs.rs/shellflip/badge.svg)](https://docs.rs/shellflip)

[English ](./README.md/) |  **中文版**

Rust 中的优雅进程重启。

这个库促进了服务的升级或重新配置，而不会中断现有的连接。这是通过分叉进程并在旧进程与新进程之间传递少量状态来实现的；一旦新进程成功启动，旧进程就可以终止。

该库具有以下目标：

* 成功升级（以及随之而来的旧进程不可避免地关闭）后，不再有旧代码运行。
* 新进程有一个初始化的宽限期。
* 初始化期间崩溃是可以接受的。
* 并行运行的升级只有一个。
* 发起升级的用户/进程可以知道升级是否成功。

受到 [tableflip](https://github.com/cloudflare/tableflip) Go 包的启发，但并不是直接替代品。

# 使用库

一个完整的示例在 [restarter 示例服务](examples/restarter.rs) 中提供。

主要感兴趣的结构体是 `RestartConfig`，它提供了检测或触发重启的方法。对于关闭已重启的进程，`ShutdownCoordinator` 提供了向派生任务发送关闭信号以及等待它们完成的方法。

## 许可证

采用 BSD 许可。详情请参阅 [LICENSE](LICENSE) 文件。
