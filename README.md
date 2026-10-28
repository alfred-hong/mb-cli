# mb-rs

A fast and reliable Modbus TCP and RTU client/server command-line tool. Built on top of [tokio-modbus](https://github.com/slowtec/tokio-modbus).

https://github.com/alfred-hong/mb-rs

The definitive hub for all standard specifications is the Modbus Organization, an independent group of users and suppliers that governs the protocol.
• Primary Web Portal: www.modbus.org
• [Documentation](https://www.modbus.org/modbus-specifications)

## Features

- **Modbus TCP** and **RTU** support with automatic protocol detection
- **Client**: `mb_client` is the command-line ModScan. It polls the four data areas, redraws in place, logs each sample, prints the frames, and can send one raw function code. It also runs diagnostics, file records, FIFO, mask write, and device identification
- **Server**: `mb_server` is a memory server for Modbus TCP, RTU, and RTU over TCP. With no arguments it listens on `0.0.0.0:502` and answers station 1
- **Addresses**: Decimal PDU addresses, `0x` hexadecimal PDU addresses, and classic `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` references
- **Decoding**: `--decode` replaces the raw register table. Formats are binary, hexadecimal, ANSI, 16/32/64-bit integers, and IEEE 754 `f32`/`f64`, with byte swap and MSRF/LSRF register order
- **Live read**: `--timeout` repeats a read and redraws that sample in place. `--interval` is the gap in milliseconds (default 1000). A failed sample stays on screen and the scan continues. Each repeating sample ends with `requests=N valid=M`. `--log` appends one line per sample. `--response-timeout` is the per-request wait in seconds (default 5). `--notation modicon` prints `00001` / `10001` / `30001` / `40001`
- **Table output**: One address/value table. Without `--decode` the values are decimal words. With `--decode` the values are that interpretation

## Installation

```bash
cargo install --git https://github.com/alfred-hong/mb-rs --locked
```

The package name is `mb-rs`. That one install provides both commands, `mb_client` and `mb_server`.

```bash
mb_client completions bash
mb_server completions bash
```

`completions` also accepts `zsh`, `fish`, `powershell`, and `elvish`. Both commands print the script and exit.

### From Source

```bash
git clone https://github.com/alfred-hong/mb-rs
cd mb-rs

cargo install --path .
```

### Prerequisites

- Rust 1.85+ with Cargo
- For RTU: Access to serial ports (may require permissions)

## Usage

### Basic Examples

#### Read Operations

`--unit` is the register or coil address. `--modbus_address` and `--addr` are the same station-address flag. A client uses station 0 when that flag is omitted. `--qty` is how many consecutive items to read from `--unit`. The default is 1.

```bash
# Read holding registers from TCP server. Station defaults to 0.
mb_client read holding --ip 192.168.1.100 --unit 0 --qty 10

# Same read, naming the station with either flag
mb_client read holding --ip 192.168.1.100 --modbus_address 1 --unit 0 --qty 10
mb_client read holding --ip 192.168.1.100 --addr 1 --unit 0 --qty 10

# Read the other three areas
mb_client read coil --ip 192.168.1.100 --addr 1 --unit 00001 --qty 8
mb_client read discrete --ip 192.168.1.100 --addr 1 --unit 10001 --qty 8
mb_client read input --ip 192.168.1.100 --addr 1 --unit 30001 --qty 4

# Read coils from an RTU device
mb_client read coil --device /dev/ttyUSB0 --addr 1 --unit 0 --qty 8 --baud 9600

# Read with verbose output. The extra column is hexadecimal.
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 100 --qty 5 --verbose

# With --decode, the table is the interpretation.
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode i32

# Keep reading for 10 seconds. Each sample redraws in place, then the command exits.
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 20 --timeout 10

# Decode while the table redraws in place
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --decode bin

# Keep reading until interrupted. 0 and a negative value do the same thing.
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --timeout 0
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --timeout -1
```

#### Write Operations

```bash
# Write single coil
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1

# Write multiple coils
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1,0,1,1

# Write holding register
mb_client write holding --ip 192.168.1.100 --unit 100 --value 42

# Write multiple registers
mb_client write holding --ip 192.168.1.100 --unit 100 --value 42,43,44
```

#### Server Mode

```bash
# Start TCP server on 0.0.0.0:502. Station defaults to 1.
mb_server

# Answer stations 1 and 2. --addr is the same flag. The list is fixed at startup.
mb_server --port 5020 --modbus_address 1 --modbus_address 2

# Idle TCP connections close after 12 seconds
mb_server --ip 127.0.0.1 --port 5020 --timeout 12 --verbose

# Start RTU server
mb_server --device /dev/ttyUSB0 --baud 9600 --parity even --stop-bits 1 --data-bits 8 --addr 1

# RTU frames on a TCP port
mb_server --rtu-over-tcp --port 5020

# Load a snapshot. A clean stop rewrites this file.
mb_server --csv map.csv

# Delay the response, or force function 6 to return exception 2 without writing
mb_server --delay-ms 50 --exception-function 6 --exception-code 2

# Accept writes, store nothing, and send no response
mb_server --silence-writes

# Server with a custom memory layout and a CSV snapshot
mb_server --num-coils 1000 --num-discrete 1000 --num-holding 500 --num-input 500 --csv map.csv
```

### Command Reference

#### Global Options

- `--timeout <SECONDS>` on a **read** command is how long to keep reading. Omit it and the command reads once. `10` repeats the read for 10 seconds, then exits. `0` or a negative value repeats until you interrupt it. On a terminal each sample replaces the previous one in the same place. Samples are `--interval` milliseconds apart (default 1000). Output sent to a pipe or a file keeps every sample. Connecting, and each individual request, stop after `--response-timeout` seconds when the peer stays silent (default 5).
- Server `--timeout <seconds>` is separate (default: 5). It covers binding the TCP port, opening the serial port, and idle TCP connections.
- `--verbose` / `-v` - Enable verbose output. On the server each line has the function code, station id, start address, quantity, outcome, and, for TCP, the client address.
- `--retries <COUNT>` - Extra attempts after a timeout or a broken connection. See [Retries](#retries).
- `--modbus_address <id>` - Modbus station address (default: 0 for the client, 1 for the server). `--addr` is the same flag. These flags, and `--verbose`, `--notation`, `--response-timeout`, and `--retries`, follow the client subcommand (`read`, `write`, `diagnostics`, `raw`, or `read-write`). Repeat `--modbus_address` on the server to serve more than one station. Those stations share one memory image and the list stays fixed for the process. Station 0 is the broadcast address and cannot be selected as a served station. A broadcast write updates the shared image and no response is sent; a broadcast read gets no response. Any other id that was not listed gets no response.

#### Retries

`--retries` applies to opening the connection and to every read or write. The default is 2. That is the first try plus two more, three tries in all, and then the command stops. `0` sends the request once. There is no extra wait between attempts. Each attempt still stops after `--response-timeout` seconds (default 5). The message from the last failed attempt is printed once.

A Modbus exception, such as `IllegalDataAddress`, is the device's answer and is not sent again.

Station 0 is not retried. The specification gives that address no response. Sending a write to station 0 again would store it again.

A repeating `--timeout` scan retries inside each sample. When those attempts are used up, the failed sample stays on screen and the scan continues.

Opening the connection is a new attempt each time. A refused or timed-out open is tried again, up to three times. A request retry uses the connection that is already open. If that socket closes after the request has been sent, the remaining attempts fail immediately and do not open another connection.

A write whose response was lost may already be stored. A retry sends that write again.

#### TCP Options

- `--ip <address>` - IP address to connect to or bind to
- `--port <port>` - TCP port (default: 502)

#### RTU Options  

- `--device <path>` - Serial device path (e.g., /dev/ttyUSB0, COM1)
- `--baud <rate>` - Baud rate (default: 9600)
- `--parity <none|even|odd>` - Parity (default: none)
- `--stop-bits <1|2>` - Stop bits (default: 1)
- `--data-bits <5|6|7|8>` - Data bits (default: 8)

#### Data areas

PDU addresses are 0-based. A classic reference is 1-based inside its area, so reference `40001` is holding-register PDU address 0.

| Command | Also accepted | Area | Access | Address prefix |
| --- | --- | --- | --- | --- |
| `coil` | `discrete-output`, `0xxxx` | 1st, coils / discrete outputs (线圈 / 离散输出) | read/write bits | `0XXXX` |
| `discrete` | `discrete-input`, `1xxxx` | 2nd, discrete inputs (离散输入) | read-only bits | `1XXXX` |
| `input` | `input-register`, `3xxxx` | 3rd, input registers (输入寄存器) | read-only 16-bit | `3XXXX` |
| `holding` | `holding-register`, `4xxxx` | 4th, holding registers (保持寄存器) | read/write 16-bit | `4XXXX` |

`mb_client write discrete` and `mb_client write input` stop immediately, including when `--unit` and `--value` are present. Those tables are read-only.

#### Addresses and values

`--unit` accepts three forms:

- Decimal PDU address: `0`, `100`, `40000`. A number that is not a 5- or 6-digit area reference is the 0-based PDU address.
- Hexadecimal PDU address: `0x64`, `0X00FF`.
- Area reference. Five digits use an offset of 1-9999 (`00001`, `10001`, `30001`, `40001`, `40100`). Six digits use an offset of 1-65536 (`400001`, `465536`). The PDU address is the offset minus 1. A reference whose prefix belongs to another area is rejected. An offset of 0, such as `40000`, is not a reference and stays a decimal PDU address.

Holding PDU 0 can be written `0`, `0x0`, `0x00`, `40001`, or `400001`. Holding PDU 1 can be written `1`, `0x1`, `0x01`, `40002`, or `400002`. `0x1` and `0x01` are the same hexadecimal PDU address. A classic reference uses its own area prefix: `00001` is coil PDU 0, `10001` is discrete PDU 0, and `30001` is input PDU 0.

`--value`, `--and`, `--or`, and `--data` accept decimal or `0x` hexadecimal words. Quantities stay decimal.

```bash
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 40001
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 400001
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0x01
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 40100
mb_client read coil --ip 127.0.0.1 --addr 1 --unit 00001
mb_client read coil --ip 127.0.0.1 --addr 1 --unit 0x10 --qty 8
mb_client read 0xxxx --ip 127.0.0.1 --addr 1 --unit 0 --qty 8
mb_client read discrete-input --ip 127.0.0.1 --addr 1 --unit 10001
mb_client read input-register --ip 127.0.0.1 --addr 1 --unit 30001
mb_client read holding-register --ip 127.0.0.1 --addr 1 --unit 40002
mb_client write holding --ip 127.0.0.1 --addr 1 --unit 100 --value 0x2A
```

`--unit` is the register address. `--modbus_address` and `--addr` are the station address. The built-in server answers station 1 unless that flag changes it. Client commands use station 0 unless you pass `--addr 1`.

#### Decoding register values

These flags are accepted by `mb_client read holding`, `mb_client read input`, `mb_client read fifo`, `mb_client read file`, and `mb_client read-write`. The count line stays. Without `--decode` the table shows decimal words, and `--verbose` adds a hex column there. With `--decode` that place shows the requested interpretation. Repeat `--decode` to print one decoded table per format.

`--swap-bytes` swaps the two bytes inside every 16-bit register before that interpretation. It applies to every format in the table. `--order` changes register order only for values that use more than one register.

| `--order` | Meaning |
| --- | --- |
| `msrf` | Most significant register first. The first register holds the high bits. This is the default. |
| `lsrf` | Least significant register first. The registers are reversed. |

| `--decode` | Meaning | Registers |
| --- | --- | --- |
| `bin` | binary (二进制) | 1 |
| `hex` | hexadecimal (十六进制) | 1 |
| `ansi` | two characters, high byte first (ANSI 字符). Bytes `0x20`–`0x7E` and `0xA0`–`0xFF` are Latin-1. Every other byte is `.` | 1 |
| `u16` / `i16` | unsigned / signed 16-bit integer (无符号 / 有符号整数) | 1 |
| `u32` / `i32` | unsigned / signed 32-bit integer | 2 |
| `u64` / `i64` | unsigned / signed 64-bit integer | 4 |
| `f32` / `f64` | IEEE 754 32-bit / 64-bit decimal (小数). Very small and very large values use scientific notation. | 2 / 4 |

`f32` and `f64` are IEEE 754 values, so the sign bit is part of the encoding. There is no separate unsigned 32-bit or 64-bit floating-point format. A trailing group that does not fill the format is reported and left undecoded. `--swap-bytes` and `--order lsrf` require at least one `--decode`.

```bash
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode bin
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode hex
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode u16
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode i16
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode i32 --decode f32
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode u64 --decode i64 --decode f64
mb_client read input --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode u32 --order lsrf --swap-bytes
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode ansi --swap-bytes
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --decode bin
mb_client read fifo --ip 127.0.0.1 --addr 1 --unit 1 --decode i16
mb_client read file --ip 127.0.0.1 --addr 1 --group 4:7:2 --decode u32
mb_client read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

#### Read commands

```bash
mb_client read <area> --unit <register> [--modbus_address <station>] [--qty <quantity>] [--timeout <SECONDS>] [--interval <MILLISECONDS>] [--log <FILE>] [--notation pdu|modicon] [--response-timeout <SECONDS>] [--retries <COUNT>] [--decode <format>...] [--order msrf|lsrf] [--swap-bytes] [connection options]
```

- `--qty` defaults to 1 and is a decimal number. Coils and discrete inputs (FC 01/02) accept 1-2000. Holding and input registers (FC 03/04) accept 1-125. `0`, a number above that limit, or a non-decimal quantity is rejected before a connection is opened. The message names the accepted range, for example `quantity 2001 is outside 1-2000 (FC 01/02)`.
- `--decode`, `--order`, and `--swap-bytes` apply to `holding` and `input`. Coil and discrete reads do not take them. FIFO, file, and `read-write` accept the same three flags. A decoded read prints the interpretation in place of the raw register table.
- `read fifo` uses `--unit` as the FIFO pointer, a holding-register address. The register at that address holds the queue count, and the registers after it are the queued values. The count is at most 31. A count above 31 returns exception 03 (`IllegalDataValue`). The read does not clear the queue.
- `read file` repeats `--group FILE:RECORD:LENGTH`. The file number is 1-65535. The record is 0-9999 (`0x270F`), and the length must stay inside that range. `1:10000:1` is rejected before a connection is opened, with `file record 10000 is outside 0-9999 (0x270F)`.
- `--timeout` repeats that read and redraws the sample in place. It is not accepted by writes. `mb_client read-write` still runs once, because repeating it would write again.
- `--interval` is the milliseconds from one sample to the next. The default is 1000. `0` sends the next request as soon as the previous one finishes. A failed sample during `--timeout` stays on screen and the scan continues. Each repeating sample ends with `requests=N valid=M`.
- `--log FILE` appends one line per sample: a UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`, then the sample text with line breaks replaced by ` | `.
- `--notation modicon` prints `00001`, `10001`, `30001`, and `40001` instead of the protocol address. Offsets above 9999 use six digits, so protocol address 9999 is `410000`.
- `--response-timeout` is the seconds allowed for the connection and for each response. The default is 5. It is separate from `--timeout`.
- `--retries` is how many extra attempts follow a timeout or a broken connection. The default is 2. See [Retries](#retries).
- `--verbose` prints the bytes of each transmitted and received frame, prefixed `tx` and `rx`.

```bash
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --interval 500
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --log scan.log
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --notation modicon
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --response-timeout 2
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --verbose
mb_client raw --ip 127.0.0.1 --addr 1 --function 3 --data 00,00,00,01
```

`mb_client raw` sends one function code from 1 to 127 and prints the response. `--function` is decimal or `0x`. `--data` is the bytes after the function code, written as hex and separated by commas or spaces. An empty `--data` sends no extra bytes. `--data` does not apply the `--qty` limits. A raw read whose quantity is 0, whose register quantity is 126, or whose coil quantity is 2001 is sent, and the server returns exception 03. For `--function 5`, the value word in `--data` must be `FF,00` (ON) or `00,00` (OFF). Any other word is rejected before connecting, with `coil value must be FF00 or 0000`.

Other reads:

```bash
mb_client read exception-status --ip 127.0.0.1 --addr 1
mb_client read event-counter --ip 127.0.0.1 --addr 1
mb_client read event-log --ip 127.0.0.1 --addr 1
mb_client read server-id --ip 127.0.0.1 --addr 1
mb_client read fifo --ip 127.0.0.1 --addr 1 --unit 1
mb_client read file --ip 127.0.0.1 --addr 1 --group 4:1:2 --group 3:9:2
mb_client read device-id --ip 127.0.0.1 --addr 1 --code basic
mb_client read device-id --ip 127.0.0.1 --addr 1 --code regular
mb_client read device-id --ip 127.0.0.1 --addr 1 --code extended
mb_client read device-id --ip 127.0.0.1 --addr 1 --code specific --object 0
```

`--code` is `basic`, `regular`, `extended`, or `specific`. It defaults to `basic`. `--object` defaults to 0. For `specific`, `--object` is the one object to return, from 0 to 255. For the other three codes, `--object` is the first object of the stream. If that object is not a known object, the server answers as if `--object 0` had been used. An unknown `specific` object returns exception 02 (`IllegalDataAddress`). The memory server reports conformity `0x82` (regular identification, stream and individual access). An `extended` request is answered at that level, objects 0x00-0x06. CANopen general reference (FC 43/13) is not implemented.

Objects returned by this server:

| Id | Name | Value |
| --- | --- | --- |
| 0x00 | VendorName | `mb-rs` |
| 0x01 | ProductCode | `mb` |
| 0x02 | MajorMinorRevision | the crate version |
| 0x03 | VendorUrl | `https://github.com/alfred-hong/mb-rs` |
| 0x04 | ProductName | `mb-rs` |
| 0x05 | ModelName | `memory-server` |
| 0x06 | UserApplicationName | empty |

`basic` returns 0x00–0x02. `regular` and `extended` return 0x00–0x06.

#### Write commands

```bash
mb_client write <area> --unit <register> [--modbus_address <station>] [--response-timeout <SECONDS>] [--retries <COUNT>] --value <values> [connection options]
```

- One coil uses FC 05. `--value 0` is sent as `0x0000` (OFF). Any other word, such as `1` or `0x0002`, is sent as `0xFF00` (ON). Several coils use FC 15. `--value` then takes 2-1968 comma-separated words, and the same 0 / non-zero rule applies to each word. More than 1968 values is rejected before connecting.
- On TCP, a function 05 frame whose value is not `0x0000` or `0xFF00` returns exception 03 and leaves the coil unchanged. `write coil` does not send those other values. `raw --function 5` rejects them before connecting.
- One holding register uses FC 06. Several use FC 16, at most 123 values. `--value` words are decimal or `0x` hex, from 0 to 65535.
- Station 0 (`--addr 0` or `--modbus_address 0`) is broadcast. A write is stored and no response is sent, so the client times out. That address is not retried. `--retries 0` makes the single wait equal to `--response-timeout`.
- Mask write (FC 22) applies `(current AND --and) OR (--or AND NOT --and)`. Both masks are required. With the register holding `0x0012`, the specification example is:

```bash
mb_client write holding --ip 127.0.0.1 --addr 1 --unit 4 --value 0x0012
mb_client write mask --ip 127.0.0.1 --addr 1 --unit 4 --and 0x00F2 --or 0x0025
```

The register then reads `0x0017`.

- File records (FC 21). Each `--group` is `FILE:RECORD:v1,v2`. The file number is 1-65535. Records run from 0 through 9999 (`0x270F`), including every value in the group. A record of 10000, or a span past 9999, is rejected before connecting:

```bash
mb_client write file --ip 127.0.0.1 --addr 1 --group 4:7:0x1234,2
```

- Read/write multiple registers (FC 23) writes first, then reads. `--value` is 1-121 words. `--read-qty` is 1-125 and defaults to 1. `--read-unit` is the first register to read. `--unit` is the first register to write. `--decode` prints only the interpretation of the values just read:

```bash
mb_client read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

#### Diagnostics

FC 08 is a serial-line function. The memory server also answers it on TCP.

```bash
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub return-query --data 0x1234
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub echo --data 0x1234
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub restart --data 0xFF00
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub ascii-delimiter --data 0x0A00
```

`--sub` accepts a name or a sub-function number. `echo` is another name for `return-query`. For `ascii-delimiter`, the character is the high byte of `--data`, so `0x0A00` sets the delimiter to line feed.

| `--sub` | Number |
| --- | --- |
| `return-query`, `echo` | 0 |
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

`listen-only` produces no response, so the client reports a timeout. `restart` with data `0xFF00` clears the counters and the event log. Functions 07, 11, 12, and 17 are marked serial-line only in the specification; this server also answers them on TCP.

#### Function coverage

| Code | Command | Notes |
| --- | --- | --- |
| 01 | `mb_client read coil` | 1-2000 coils |
| 02 | `mb_client read discrete` | 1-2000 discrete inputs |
| 03 | `mb_client read holding` | 1-125 holding registers |
| 04 | `mb_client read input` | 1-125 input registers |
| 05 / 15 | `mb_client write coil` | one coil, or 2-1968 coils |
| 06 / 16 | `mb_client write holding` | one register, or 2-123 registers |
| 07 | `mb_client read exception-status` | first 8 coils, bit 0 is address 0 |
| 08 | `mb_client diagnostics` | serial-line diagnostics |
| 11 | `mb_client read event-counter` | status and event count |
| 12 | `mb_client read event-log` | newest event first, 64 retained |
| 17 | `mb_client read server-id` | station address, run status, `mb-rs` |
| 20 | `mb_client read file` | file 1-65535, records 0-0x270F, reference type 6 |
| 21 | `mb_client write file` | echoes the request |
| 22 | `mb_client write mask` | one holding register |
| 23 | `mb_client read-write` | write, then read |
| 24 | `mb_client read fifo` | count in the pointer register, at most 31 values |
| 43 / 14 | `mb_client read device-id` | MEI type 14 |
| 43 / 13 | — | CANopen general reference is not implemented |

A quantity of 0 or a quantity above the function limit returns exception 03 (illegal data value). An address outside the configured table returns exception 02 (illegal data address). An unknown function returns exception 01. On TCP, function 05 accepts only `0x0000` (OFF) and `0xFF00` (ON). Any other value returns exception 03 and leaves the coil unchanged. Reserved and user-defined function codes are not implemented. File records start at 0 until written. The FIFO read does not clear the queue. A FIFO count above 31 returns exception 03. An unknown device-identification object restarts a stream at object 0, and an unknown individual object returns exception 02.

#### Specification coverage

Application behavior follows MODBUS Application Protocol Specification V1.1b3. Serial-line framing and the TCP MBAP header are specified in the companion implementation guides, not in that document.

The client and the TCP memory server implement every public function code in V1.1b3 except 43/13. Quantity limits match the specification: coils and discrete inputs 1–2000, register reads 1–125, FC 15 at most 1968 coils, FC 16 at most 123 registers, and FC 23 writes at most 121 registers.

These limits stay, because no current library can add them without dropping function codes this project already implements. tokio-modbus 0.17.0 is the newest release and is the stack in use. rs-modbus 2.0 and rmodbus 0.12 can frame ASCII and UDP, but their servers cover functions 01–06, 15, and 16 (rs-modbus also 17, 22, 23, and 43/14). They do not cover 07, 08, 11, 12, 20, 21, and 24. rodbus does not add ASCII. Switching stacks is skipped.

Skipped for this release:

- Modbus ASCII
- Modbus UDP
- RS-485 RTS around each RTU response
- A configurable RTU inter-frame gap
- Receiving FC 08, FC 20, and FC 21 on the built-in RTU server and the RTU-over-TCP server. The RTU framer rejects those function codes while guessing the frame length. Modbus TCP receives them. A client can still send them to another RTU device.
- FC 43/13 CANopen general reference. V1.1b3 assigns MEI type 13 to CiA and does not publish the payload. The server returns exception 01.

Limits that remain with tokio-modbus 0.17.0:

- Report Server ID (FC 17) is sent as a custom `0x11` PDU. `Response::ReportServerId` sizes the buffer one byte shorter than the bytes the encoder writes.
- Bus communication-error, NAK, busy, and overrun counters stay 0. The codec drops a bad frame before it becomes a request, so those events are not visible to the server.
- Exception codes 05 (acknowledge), 06 (server device busy), 08 (memory parity), 0A (gateway path unavailable), and 0B (gateway target failed to respond) are not produced. This process is a memory server, not a programming port and not a gateway, and it does not simulate a file-memory parity fault.

#### Server Command

```bash
mb_server [connection options] [memory options]
```

The server accepts the same TCP and RTU options as the client, including `--parity`, `--stop-bits`, and `--data-bits`.

Memory options, each 0-65535:
- `--num-coils <count>` - Number of coils (default: 10000). Each coil starts as a random 0 or 1.
- `--num-discrete <count>` - Number of discrete inputs (default: 10000). Discrete inputs start OFF and are not part of the CSV file.
- `--num-holding <count>` - Number of holding registers (default: 10000). Each register starts as a random integer 0-65535.
- `--num-input <count>` - Number of input registers (default: 10000). Each register starts as a random integer 0-65535.

Each start without a restored snapshot draws a new set of those integers. Discrete inputs stay OFF. A client write to a coil or holding register replaces that value, and the value stays until another client write. Input registers are not writable and do not change after startup.

`--csv <FILE>` loads `area,address,value` rows before serving. The first non-empty line must be that header. Later empty lines are skipped. `area` is `coil`, `holding`, or `input`, case-insensitive on load and lowercase when the file is rewritten. Discrete inputs are not stored. `address` uses the same forms as the client. A register value is decimal digits only, 0-65535, with no `0x` prefix. A coil value is exactly `0` or `1`. A row replaces the random value at that address, and the last row for an address wins. A clean stop, including Ctrl+C, rewrites the file with every address in those three areas, so the next start restores that image. A forced kill does not rewrite it. If binding or opening the port fails, the file is left unchanged.

The listen address, station list, snapshot path, delay, forced exception, and silence setting stay as they were at startup. A client can still write coils and holding registers. Input registers and discrete inputs stay read-only.

```csv
area,address,value
holding,0,1234
holding,40001,1
input,0,7
coil,0,1
```

`--verbose` prints one line per request, for example `station=1 fc=0x03 addr=0 qty=4 result=ok from 127.0.0.1:54321`.

`--delay-ms <MILLISECONDS>` waits after a normal request is stored and before the response is sent. `--exception-function <CODE>` with `--exception-code <2|3|4>` answers that function with the exception and does not change memory. A later request whose function is not the forced one is applied as usual. `--silence-writes` accepts a write, stores nothing, and sends no response.

An address outside the `--num-*` window returns exception 02. The CSV sets values inside that window. It does not remove addresses from it.

### Protocol Detection

The client requires `--ip` or `--device`. Both together is an error.

The server chooses the listener as follows:

- `--device` → Modbus RTU on that serial port
- `--rtu-over-tcp` → Modbus RTU frames on TCP. `--ip` defaults to `0.0.0.0` and `--port` defaults to 502
- `--ip`, or neither `--ip` nor `--device` → Modbus TCP. The default address is `0.0.0.0` port 502
- `--device` together with `--ip` or `--rtu-over-tcp` → error

### Examples by Use Case

#### Industrial Automation

```bash
# Read PLC outputs at station 1
mb_client read coil --ip 192.168.1.10 --addr 1 --unit 0 --qty 16

# Read sensor values
mb_client read input --ip 192.168.1.10 --addr 1 --unit 1000 --qty 4 --verbose

# Watch a holding block for 10 seconds
mb_client read holding --ip 192.168.1.10 --addr 1 --unit 0 --qty 4 --timeout 10 --decode i32

# Write setpoint
mb_client write holding --ip 192.168.1.10 --addr 1 --unit 2000 --value 1500
```

#### Serial Device Communication

```bash
# Read from RTU device with custom baud rate
mb_client read holding --device /dev/ttyUSB0 --baud 19200 --addr 1 --unit 0 --qty 10

# Write to RTU device
mb_client write coil --device /dev/ttyUSB0 --addr 2 --unit 0 --value 1
```

#### Testing and Development

```bash
# Start test server (Modbus address defaults to 1, TCP binds 0.0.0.0:502)
mb_server --port 5020 &

# Test client against local server
mb_client read holding --ip 127.0.0.1 --port 5020 --addr 1 --unit 0 --qty 5

# Test with verbose logging
mb_client read coil --ip 127.0.0.1 --port 5020 --addr 1 --unit 0 --qty 8 --verbose
```

## Error Handling

The tool provides detailed error messages for common issues:

- **Connection timeouts**: The message names the peer and how many seconds were allowed. After the retries are used up, that message is printed once
- **Modbus exceptions**: The exception name, such as `IllegalDataAddress`, is printed once and the request is not sent again
- **Validation errors**: A rejected quantity, address, file reference, station id, interval, coil value, or diagnostics name states the accepted range. `--qty` outside 1-2000 or 1-125, a file record outside 0-9999, more than 1968 coil values, more than 123 holding values, or a function 23 write outside 1-121 stops before a connection is opened. `raw --function 5` with a value other than `FF,00` or `00,00` stops with `coil value must be FF00 or 0000`. `--swap-bytes` and `--order lsrf` without `--decode` stop before a connection is opened. A negative number is reported as an invalid value
- **Flag order**: `--ip`, `--device`, `--addr`, and the other connection flags follow the command. Using one of them before `read` or `write` says to put it after the command
- **Read-only areas**: Writing discrete inputs or input registers stops before a connection is opened
- **Area mismatch**: A `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` reference is rejected when its prefix belongs to another command
- **Connection failures**: Network and serial port error details

## Origin

- **Path:** https://github.com/wkrettek/mb-cli
- **Description:** A fast and reliable Modbus TCP and RTU client/server command-line tool. Built on top of [tokio-modbus](https://github.com/slowtec/tokio-modbus).
- **Thanks:** Warren Krettek, for the original `mb-cli` client, memory server, and the work this repository continues.
