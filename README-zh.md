# mb-rs

Modbus TCP 与 RTU 客户端和内存服务器。一个包同时安装 `mb_client` 和 `mb_server`。基于 [tokio-modbus](https://github.com/slowtec/tokio-modbus)。

https://github.com/alfred-hong/mb-rs

英文文档：[README.md](README.md)。

协议由 Modbus 组织维护：[www.modbus.org](https://www.modbus.org)，规范索引见 [specification index](https://www.modbus.org/modbus-specifications)。

## 安装

```bash
cargo install --git https://github.com/alfred-hong/mb-rs --locked
```

从检出的源码安装：

```bash
git clone https://github.com/alfred-hong/mb-rs
cd mb-rs
cargo install --path .
```

需要 Rust 1.85 或更新版本。RTU 需要进程能够打开的串口。

补全命令打印脚本后退出。可用的 shell 为 `bash`、`zsh`、`fish`、`powershell`、`elvish`。

```bash
mb_client completions bash
mb_server completions bash
```

## 地址

`--unit` 是线圈或寄存器。`--modbus_address` 和 `--addr` 是站号，二者是同一个参数。客户端省略该参数时使用站号 0。服务器默认应答站号 1，除非该参数指定了其他站号。

`--unit` 接受三种写法：

- 从 0 起的十进制 PDU 地址。`40000` 是 PDU 地址 40000，不是区域引用，因为引用里的偏移不能为 0。
- 十六进制 PDU 地址，例如 `0x64` 或 `0X00FF`。
- 传统区域引用。五位数字覆盖偏移 1–9999（`00001`、`10001`、`30001`、`40001`）。六位数字覆盖偏移 1–65536（`400001`、`465536`）。PDU 地址等于偏移减 1。属于其他数据区的前缀会被拒绝。

保持寄存器 PDU 0 可以写成 `0`、`0x0`、`0x00`、`40001` 或 `400001`。保持寄存器 PDU 1 可以写成 `1`、`0x1`、`0x01`、`40002` 或 `400002`。`00001` 是线圈 PDU 0，`10001` 是离散输入 PDU 0，`30001` 是输入寄存器 PDU 0。

| 命令 | 也可写作 | 数据区 | 访问 | 前缀 |
| --- | --- | --- | --- | --- |
| `coil` | `discrete-output`、`0xxxx` | 线圈 / 离散输出 | 可读写位 | `0XXXX` |
| `discrete` | `discrete-input`、`1xxxx` | 离散输入 | 只读位 | `1XXXX` |
| `input` | `input-register`、`3xxxx` | 输入寄存器 | 只读 16 位字 | `3XXXX` |
| `holding` | `holding-register`、`4xxxx` | 保持寄存器 | 可读写 16 位字 | `4XXXX` |

`mb_client write discrete` 和 `mb_client write input` 在打开连接之前就停止。这两张表是只读的。

`--value`、`--and`、`--or` 和 `--data` 接受十进制或 `0x` 十六进制字，范围 0–65535。数量保持十进制。

## 客户端

`mb_client` 读取四个数据区，写入线圈和保持寄存器，并发送诊断、文件记录、FIFO、屏蔽写、设备标识，以及一个原始功能码。

连接参数写在子命令之后（`read`、`write`、`diagnostics`、`raw` 或 `read-write`）。`--ip` 和 `--device` 不能同时使用。

| 参数 | 含义 |
| --- | --- |
| `--ip <address>` | TCP 对端 |
| `--port <port>` | TCP 端口，默认 502 |
| `--device <path>` | 串口设备，例如 `/dev/ttyUSB0` 或 `COM1` |
| `--baud <rate>` | 波特率，默认 9600 |
| `--parity <none\|even\|odd>` | 校验，默认 `none` |
| `--stop-bits <1\|2>` | 停止位，默认 1 |
| `--data-bits <5\|6\|7\|8>` | 数据位，默认 8 |
| `--addr <id>` | 站号。默认 0。与 `--modbus_address` 相同 |
| `--response-timeout <SECONDS>` | 等待连接和每次响应的秒数。默认 5。与 `--timeout` 不同 |
| `--retries <COUNT>` | 超时或连接中断后的额外尝试次数。默认 2 |
| `--verbose`、`-v` | 打印每个发送和接收的帧，前缀分别为 `tx` 和 `rx` |
| `--notation <pdu\|modicon>` | 地址列。默认 `pdu`。`modicon` 打印 `00001`、`10001`、`30001`、`40001`。偏移超过 9999 时使用六位数字，因此 PDU 9999 显示为 `410000` |

### 重试

默认 `--retries 2` 是首次尝试再加两次，一共三次，然后命令停止。`0` 只发送一次。两次尝试之间没有额外等待。每次尝试仍在 `--response-timeout` 秒后停止。只打印最后一次失败的消息。

Modbus 异常（例如 `IllegalDataAddress`）是设备的回答，不会再次发送。站号 0 不重试，因为该地址没有响应，重复写入会再次保存该值。

重复的 `--timeout` 扫描在每个采样内部重试。这些尝试用完后，失败的采样留在屏幕上，扫描继续。每次打开连接都是一次新的尝试。请求重试使用已经打开的连接。如果请求发出后套接字关闭，剩余尝试立即失败。响应丢失的写入可能已经保存。重试会再次发送该写入。

### 读取

```bash
mb_client read <area> --unit <register> [--addr <station>] [--qty <quantity>] [--timeout <SECONDS>] [--interval <MILLISECONDS>] [--log <FILE>] [--notation pdu|modicon] [--response-timeout <SECONDS>] [--retries <COUNT>] [--decode <format>...] [--order msrf|lsrf] [--swap-bytes] [connection options]
```

`--qty` 默认为 1。线圈和离散输入接受 1–2000。保持寄存器和输入寄存器接受 1–125。`0`、超过该上限的数字，或非十进制数量，会在打开连接之前被拒绝。消息会写出范围，例如 `quantity 2001 is outside 1-2000 (FC 01/02)`。

`--timeout` 是持续读取的时长。省略它则只读一次。正数会重复，直到经过这么多秒。`0` 或负数会一直重复，直到被中断。在终端上，每个采样替换上一个采样。管道或文件保留每一个采样。采样间隔为 `--interval` 毫秒（默认 1000）。`0` 表示上一次请求结束后立刻发送下一次。失败的采样留在屏幕上。每个重复采样以 `requests=N valid=M` 结尾。

`--log FILE` 为每个采样追加一行：UTC 时间戳 `YYYY-MM-DDTHH:MM:SSZ`，然后是采样文本，换行替换为 ` | `。

```bash
mb_client read holding --ip 192.168.1.100 --addr 1 --unit 0 --qty 10
mb_client read coil --ip 192.168.1.100 --addr 1 --unit 00001 --qty 8
mb_client read discrete --ip 192.168.1.100 --addr 1 --unit 10001 --qty 8
mb_client read input --ip 192.168.1.100 --addr 1 --unit 30001 --qty 4
mb_client read coil --device /dev/ttyUSB0 --addr 1 --unit 0 --qty 8 --baud 9600
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 20 --timeout 10 --interval 500
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --log scan.log
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --notation modicon
```

其他读取：

```bash
mb_client read exception-status --ip 127.0.0.1 --addr 1
mb_client read event-counter --ip 127.0.0.1 --addr 1
mb_client read event-log --ip 127.0.0.1 --addr 1
mb_client read server-id --ip 127.0.0.1 --addr 1
mb_client read fifo --ip 127.0.0.1 --addr 1 --unit 1
mb_client read file --ip 127.0.0.1 --addr 1 --group 4:1:2 --group 3:9:2
mb_client read device-id --ip 127.0.0.1 --addr 1 --code basic
mb_client read device-id --ip 127.0.0.1 --addr 1 --code specific --object 0
```

`read fifo` 把 `--unit` 当作指针。该保持寄存器存放队列计数，其后的寄存器是队列中的值。计数最多为 31。更大的计数返回异常 03（`IllegalDataValue`）。读取不会清空队列。

`read file` 可重复 `--group FILE:RECORD:LENGTH`。文件号为 1–65535。记录号为 0–9999（`0x270F`），长度必须留在该范围内。`1:10000:1` 会在打开连接之前被拒绝。

`--code` 为 `basic`、`regular`、`extended` 或 `specific`，默认 `basic`。`--object` 默认为 0。对 `specific`，它选择一个对象，范围 0–255。对其他代码，它是流的第一个对象。未知的流对象会按对象 0 回答。未知的 `specific` 对象返回异常 02。内存服务器报告一致性级别 `0x82`。`extended` 请求按该级别回答，对象为 0x00–0x06。

| 编号 | 名称 | 值 |
| --- | --- | --- |
| 0x00 | VendorName | `mb-rs` |
| 0x01 | ProductCode | `mb` |
| 0x02 | MajorMinorRevision | crate 版本 |
| 0x03 | VendorUrl | `https://github.com/alfred-hong/mb-rs` |
| 0x04 | ProductName | `mb-rs` |
| 0x05 | ModelName | `memory-server` |
| 0x06 | UserApplicationName | 空 |

`basic` 返回 0x00–0x02。`regular` 和 `extended` 返回 0x00–0x06。CANopen 通用参考（FC 43/13）未实现。

### 解码

`--decode`、`--order` 和 `--swap-bytes` 适用于 `holding`、`input`、`fifo`、`file` 和 `read-write`。线圈和离散输入读取不接受这些参数。没有 `--decode` 时，表格显示十进制字，`--verbose` 额外增加一列十六进制。有 `--decode` 时，该位置显示解释结果。重复 `--decode` 会为每种格式各打印一张表。

`--swap-bytes` 在解码前交换每个 16 位寄存器内部的两个字节。`--order` 只改变占用多个寄存器的值的寄存器顺序。`--swap-bytes` 和 `--order lsrf` 至少需要一个 `--decode`。

| `--order` | 含义 |
| --- | --- |
| `msrf` | 高位寄存器在前。默认。 |
| `lsrf` | 低位寄存器在前。 |

| `--decode` | 含义 | 寄存器数 |
| --- | --- | --- |
| `bin` | 二进制 | 1 |
| `hex` | 十六进制 | 1 |
| `ansi` | 两个字符，高字节在前。字节 `0x20`–`0x7E` 和 `0xA0`–`0xFF` 按 Latin-1 显示。其他字节显示为 `.` | 1 |
| `u16` / `i16` | 无符号 / 有符号 16 位整数 | 1 |
| `u32` / `i32` | 无符号 / 有符号 32 位整数 | 2 |
| `u64` / `i64` | 无符号 / 有符号 64 位整数 | 4 |
| `f32` / `f64` | IEEE 754 的 32 位 / 64 位值。极小和极大的值使用科学计数法 | 2 / 4 |

`f32` 和 `f64` 包含符号位。没有无符号浮点格式。末尾凑不满该格式的一组会被报告，并保持不解码。

```bash
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode i32
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode i32 --decode f32
mb_client read input --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode u32 --order lsrf --swap-bytes
mb_client read fifo --ip 127.0.0.1 --addr 1 --unit 1 --decode i16
mb_client read file --ip 127.0.0.1 --addr 1 --group 4:7:2 --decode u32
```

### 写入

```bash
mb_client write <area> --unit <register> [--addr <station>] [--response-timeout <SECONDS>] [--retries <COUNT>] --value <values> [connection options]
```

单个线圈使用 FC 05。`--value 0` 发送为 `0x0000`（OFF）。其他任何字，例如 `1` 或 `0x0002`，发送为 `0xFF00`（ON）。多个线圈使用 FC 15，为 2–1968 个逗号分隔的字，规则同样是 0 为 OFF、非 0 为 ON。超过 1968 个值会在连接之前被拒绝。

在 TCP 上，功能码 05 的帧如果值不是 `0x0000` 或 `0xFF00`，返回异常 03，线圈保持不变。`write coil` 不会发送这些其他值。`raw --function 5` 在连接之前拒绝它们，消息为 `coil value must be FF00 or 0000`。

单个保持寄存器使用 FC 06。多个使用 FC 16，最多 123 个字。

站号 0 是广播。写入会被保存，但不发送响应，因此客户端超时。该地址不重试。

屏蔽写（FC 22）按 `(当前值 AND --and) OR (--or AND NOT --and)` 计算。两个掩码都必填。寄存器当前为 `0x0012` 时：

```bash
mb_client write holding --ip 127.0.0.1 --addr 1 --unit 4 --value 0x0012
mb_client write mask --ip 127.0.0.1 --addr 1 --unit 4 --and 0x00F2 --or 0x0025
```

寄存器随后读到 `0x0017`。

文件记录（FC 21）使用 `--group FILE:RECORD:v1,v2`。文件号为 1–65535。记录号从 0 到 9999（`0x270F`），组内每个值都算在内。记录号 10000，或跨度超过 9999，会在连接之前被拒绝。

```bash
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1,0,1,1
mb_client write holding --ip 192.168.1.100 --unit 100 --value 42,43,44
mb_client write file --ip 127.0.0.1 --addr 1 --group 4:7:0x1234,2
```

`read-write`（FC 23）先写后读。`--value` 为 1–121 个字。`--read-qty` 为 1–125，默认 1。`--read-unit` 是要读取的第一个寄存器。`--unit` 是要写入的第一个寄存器。`--decode` 只打印刚刚读到的值。该命令只执行一次，因为重复执行会再次写入。

```bash
mb_client read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

### 诊断

FC 08 是串行链路功能。内存服务器在 TCP 上也会应答它。

`--sub` 接受名称或数字。`echo` 是 `return-query` 的另一个名称。对 `ascii-delimiter`，字符是 `--data` 的高字节，因此 `0x0A00` 把分隔符设为换行。`listen-only` 不产生响应，客户端报告超时。`restart` 且数据为 `0xFF00` 时，清除计数器和事件日志。

```bash
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub return-query --data 0x1234
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub restart --data 0xFF00
```

| `--sub` | 编号 |
| --- | --- |
| `return-query`、`echo` | 0 |
| `restart` | 1 |
| `diagnostic-register` | 2 |
| `ascii-delimiter` | 3 |
| `listen-only` | 4 |
| `clear-counters` | 10 (0x0A) |
| `bus-messages` | 11 (0x0B) |
| `bus-comm-errors` | 12 (0x0C) |
| `bus-exceptions` | 13 (0x0D) |
| `server-messages` | 14 (0x0E) |
| `server-no-response` | 15 (0x0F) |
| `server-nak` | 16 (0x10) |
| `server-busy` | 17 (0x11) |
| `bus-overrun` | 18 (0x12) |
| `clear-overrun` | 20 (0x14) |

### 原始功能码

`mb_client raw` 发送一个 1 到 127 的功能码。`--function` 为十进制或 `0x`。`--data` 是功能码之后的字节，写成十六进制，用逗号或空格分隔。空的 `--data` 不发送额外字节。`--data` 不应用 `--qty` 的限制。数量为 0、寄存器数量为 126，或线圈数量为 2001 的原始读取会被发出，服务器返回异常 03。对 `--function 5`，值字必须是 `FF,00` 或 `00,00`。

```bash
mb_client raw --ip 127.0.0.1 --addr 1 --function 3 --data 00,00,00,01
```

## 服务器

`mb_server` 是 Modbus TCP、RTU 和 RTU over TCP 的内存服务器。不带参数时监听 `0.0.0.0:502`，并应答站号 1。

```bash
mb_server
mb_server --port 5020 --modbus_address 1 --modbus_address 2
mb_server --ip 127.0.0.1 --port 5020 --timeout 12 --verbose
mb_server --device /dev/ttyUSB0 --baud 9600 --parity even --stop-bits 1 --data-bits 8 --addr 1
mb_server --rtu-over-tcp --port 5020
mb_server --csv map.csv
mb_server --delay-ms 50 --exception-function 6 --exception-code 2
mb_server --silence-writes
mb_server --num-coils 1000 --num-discrete 1000 --num-holding 500 --num-input 500 --csv map.csv
```

服务器使用与客户端相同的 TCP 和 RTU 参数。`--timeout` 与客户端的扫描窗口不同。默认 5 秒。它覆盖绑定 TCP 端口、打开串口，以及空闲的 TCP 连接。

重复 `--modbus_address` 或 `--addr` 可服务多个站号。这些站号共享同一块内存映像，列表在进程运行期间保持不变。站号 0 是广播地址，不能被选为服务站。广播写入更新共享映像，不发送响应。广播读取得不到响应。未列出的其他站号也得不到响应。

监听方式按下面选择：

- `--device` 在该串口上打开 Modbus RTU。
- `--rtu-over-tcp` 在 TCP 上提供 RTU 帧。`--ip` 默认 `0.0.0.0`，`--port` 默认 502。
- `--ip`，或者既没有 `--ip` 也没有 `--device`，提供 Modbus TCP。默认地址是 `0.0.0.0` 端口 502。
- `--device` 与 `--ip` 或 `--rtu-over-tcp` 同时使用是错误。

客户端必须提供 `--ip` 或 `--device`。两者同时使用是错误。

每个内存数量为 0–65535。默认都是 10000。

- `--num-coils` 把每个线圈初始化为随机的 0 或 1。
- `--num-discrete` 把每个离散输入初始化为 OFF。离散输入不写入 CSV 文件。
- `--num-holding` 和 `--num-input` 把每个寄存器初始化为 0–65535 的随机整数。

没有恢复快照时，每次启动都会重新抽取这些整数。离散输入保持 OFF。客户端对线圈或保持寄存器的写入会替换该值，直到下一次写入。输入寄存器和离散输入保持只读。

`--csv <FILE>` 在开始服务前加载 `area,address,value` 行。第一个非空行必须是该表头。之后的空行会被跳过。`area` 为 `coil`、`holding` 或 `input`，加载时不区分大小写，重写文件时使用小写。`address` 使用与客户端相同的写法。寄存器值只能是十进制数字，0–65535，不能带 `0x` 前缀。线圈值必须正好是 `0` 或 `1`。同一地址的最后一行生效。正常停止（包括 Ctrl+C）会用这三个数据区的全部地址重写该文件。强制杀死进程不会重写。绑定或打开端口失败时，文件保持不变。

```csv
area,address,value
holding,0,1234
holding,40001,1
input,0,7
coil,0,1
```

`--num-*` 窗口之外的地址返回异常 02。CSV 只设置窗口内的值，不会从窗口中去掉地址。

`--verbose` 为每个请求打印一行：`station=1 fc=0x03 addr=0 qty=4 result=ok from 127.0.0.1:54321`。`--delay-ms` 在正常请求保存之后、发送响应之前等待。`--exception-function` 与 `--exception-code` 2、3 或 4 一起使用时，用该异常回答对应功能，并且不改变内存。其他功能照常执行。`--silence-writes` 接受写入，不保存任何内容，也不发送响应。

监听地址、站号列表、快照路径、延迟、强制异常和静默设置在启动后保持不变。

## 功能码

应用行为遵循 MODBUS Application Protocol Specification V1.1b3。串行链路成帧和 TCP MBAP 头在配套的实现指南中规定。

客户端和 TCP 内存服务器实现了 V1.1b3 中除 43/13 以外的全部公开功能码。数量限制与规范一致：线圈和离散输入 1–2000，寄存器读取 1–125，FC 15 最多 1968 个线圈，FC 16 最多 123 个寄存器，FC 23 写入最多 121 个寄存器。

| 功能码 | 命令 | 说明 |
| --- | --- | --- |
| 01 | `mb_client read coil` | 1–2000 个线圈 |
| 02 | `mb_client read discrete` | 1–2000 个离散输入 |
| 03 | `mb_client read holding` | 1–125 个保持寄存器 |
| 04 | `mb_client read input` | 1–125 个输入寄存器 |
| 05 / 15 | `mb_client write coil` | 一个线圈，或 2–1968 个线圈 |
| 06 / 16 | `mb_client write holding` | 一个寄存器，或 2–123 个寄存器 |
| 07 | `mb_client read exception-status` | 前 8 个线圈，位 0 是地址 0 |
| 08 | `mb_client diagnostics` | 串行链路诊断 |
| 11 | `mb_client read event-counter` | 状态和事件计数 |
| 12 | `mb_client read event-log` | 最新事件在前，保留 64 条 |
| 17 | `mb_client read server-id` | 站号、运行状态、`mb-rs` |
| 20 | `mb_client read file` | 文件号 1–65535，记录 0–0x270F，引用类型 6 |
| 21 | `mb_client write file` | 回显请求 |
| 22 | `mb_client write mask` | 一个保持寄存器 |
| 23 | `mb_client read-write` | 先写后读 |
| 24 | `mb_client read fifo` | 计数在指针寄存器中，最多 31 个值 |
| 43 / 14 | `mb_client read device-id` | MEI 类型 14 |
| 43 / 13 | — | 未实现 CANopen 通用参考 |

数量为 0 或超过该功能上限时返回异常 03（非法数据值）。配置表之外的地址返回异常 02（非法数据地址）。未知功能返回异常 01。在 TCP 上，功能码 05 只接受 `0x0000`（OFF）和 `0xFF00`（ON）。其他值返回异常 03，线圈保持不变。保留功能码和用户自定义功能码未实现。文件记录在写入之前从 0 开始。FIFO 读取不会清空队列。FIFO 计数超过 31 返回异常 03。未知的设备标识流对象会从对象 0 重新开始，未知的单个对象返回异常 02。

规范把功能码 07、11、12 和 17 标为仅用于串行链路。本服务器在 TCP 上也会应答它们。

下列项目留在本版本之外，因为现有库无法在保留本项目已经实现的功能码的同时加入它们。当前使用的协议栈是 tokio-modbus 0.17.0。rs-modbus 2.0 和 rmodbus 0.12 可以成帧 ASCII 和 UDP，但它们的服务器只覆盖功能码 01–06、15 和 16（rs-modbus 另外覆盖 17、22、23 和 43/14）。它们不覆盖 07、08、11、12、20、21 和 24。rodbus 不提供 ASCII。

- Modbus ASCII
- Modbus UDP
- 每条 RTU 响应前后的 RS-485 RTS
- 可配置的 RTU 帧间间隔
- 在内置 RTU 服务器和 RTU-over-TCP 服务器上接收 FC 08、FC 20 和 FC 21。RTU 成帧器在猜测帧长时拒绝这些功能码。Modbus TCP 可以接收它们。客户端仍可以把它们发给另一台 RTU 设备。
- FC 43/13 CANopen 通用参考。V1.1b3 把 MEI 类型 13 分配给 CiA，且未公布载荷。服务器返回异常 01。

仍受 tokio-modbus 0.17.0 限制的部分：

- 报告服务器标识（FC 17）以自定义 `0x11` PDU 发送。`Response::ReportServerId` 分配的缓冲区比编码器写入的字节少一个字节。
- 总线通信错误、NAK、忙和超限计数保持为 0。编解码器在坏帧成为请求之前就丢弃它，因此服务器看不到这些事件。
- 不产生异常码 05（确认）、06（服务器设备忙）、08（存储器奇偶校验）、0A（网关路径不可用）和 0B（网关目标设备响应失败）。本进程是内存服务器，不是编程口，也不是网关。

## 错误

- **连接超时。** 消息写明对端和允许的秒数。重试用完后，该消息只打印一次。
- **Modbus 异常。** 异常名称（例如 `IllegalDataAddress`）只打印一次，请求不会再次发送。
- **参数校验。** 被拒绝的数量、地址、文件引用、站号、间隔、线圈值或诊断名称会写明可接受的范围。`--qty` 超出 1–2000 或 1–125、文件记录超出 0–9999、线圈值超过 1968 个、保持寄存器值超过 123 个，或功能码 23 的写入超出 1–121，都会在打开连接之前停止。`raw --function 5` 的值不是 `FF,00` 或 `00,00` 时，以 `coil value must be FF00 or 0000` 停止。没有 `--decode` 时使用 `--swap-bytes` 或 `--order lsrf`，会在打开连接之前停止。负数会被报告为无效值。
- **参数顺序。** `--ip`、`--device`、`--addr` 和其他连接参数写在命令之后。把它们写在 `read` 或 `write` 之前时，会提示放到命令后面。
- **只读数据区。** 写入离散输入或输入寄存器会在打开连接之前停止。
- **数据区不匹配。** `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` 引用的前缀属于另一个命令时会被拒绝。
- **连接失败。** 网络和串口错误包含系统给出的细节。

## 来源

- **路径：** https://github.com/wkrettek/mb-cli
- **说明：** 快速可靠的 Modbus TCP 与 RTU 客户端/服务器命令行工具。基于 [tokio-modbus](https://github.com/slowtec/tokio-modbus)。
- **致谢：** Warren Krettek，感谢最初的 `mb-cli` 客户端、内存服务器，以及本仓库所延续的工作。
