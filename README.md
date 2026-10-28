# mb-rs

Modbus TCP and RTU client and memory server. One package installs both commands, `mb_client` and `mb_server`. Built on [tokio-modbus](https://github.com/slowtec/tokio-modbus).

https://github.com/alfred-hong/mb-rs

Chinese documentation: [README-zh.md](README-zh.md).

The protocol is maintained by the Modbus Organization: [www.modbus.org](https://www.modbus.org) and the [specification index](https://www.modbus.org/modbus-specifications).

## Installation

```bash
cargo install --git https://github.com/alfred-hong/mb-rs --locked
```

From a checkout:

```bash
git clone https://github.com/alfred-hong/mb-rs
cd mb-rs
cargo install --path .
```

Rust 1.85 or newer is required. RTU needs a serial port the process can open.

Shell completions print a script and exit. The shell is `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

```bash
mb_client completions bash
mb_server completions bash
```

## Addresses

`--unit` is the coil or register. `--modbus_address` and `--addr` are the station. They are the same flag. The client uses station 0 when the flag is omitted. The server answers station 1 unless the flag names other stations.

`--unit` accepts three forms:

- A decimal PDU address, starting at 0. `40000` is PDU address 40000, not a reference, because the offset inside a reference cannot be 0.
- A hexadecimal PDU address, such as `0x64` or `0X00FF`.
- A classic area reference. Five digits cover offsets 1-9999 (`00001`, `10001`, `30001`, `40001`). Six digits cover offsets 1-65536 (`400001`, `465536`). The PDU address is the offset minus 1. A prefix from another area is rejected.

Holding PDU 0 is `0`, `0x0`, `0x00`, `40001`, or `400001`. Holding PDU 1 is `1`, `0x1`, `0x01`, `40002`, or `400002`. `00001` is coil PDU 0, `10001` is discrete PDU 0, and `30001` is input PDU 0.

| Command | Also accepted | Area | Access | Prefix |
| --- | --- | --- | --- | --- |
| `coil` | `discrete-output`, `0xxxx` | coils / discrete outputs | read/write bits | `0XXXX` |
| `discrete` | `discrete-input`, `1xxxx` | discrete inputs | read-only bits | `1XXXX` |
| `input` | `input-register`, `3xxxx` | input registers | read-only 16-bit words | `3XXXX` |
| `holding` | `holding-register`, `4xxxx` | holding registers | read/write 16-bit words | `4XXXX` |

`mb_client write discrete` and `mb_client write input` stop before a connection is opened. Those tables are read-only.

`--value`, `--and`, `--or`, and `--data` accept decimal or `0x` hexadecimal words, 0-65535. Quantities stay decimal.

## Client

`mb_client` reads the four data areas, writes coils and holding registers, and sends diagnostics, file records, FIFO, mask write, device identification, and one raw function code.

Connection flags follow the subcommand (`read`, `write`, `diagnostics`, `raw`, or `read-write`). `--ip` and `--device` cannot be combined.

| Flag | Meaning |
| --- | --- |
| `--ip <address>` | TCP peer |
| `--port <port>` | TCP port, default 502 |
| `--device <path>` | Serial device, for example `/dev/ttyUSB0` or `COM1` |
| `--baud <rate>` | Baud rate, default 9600 |
| `--parity <none\|even\|odd>` | Parity, default `none` |
| `--stop-bits <1\|2>` | Stop bits, default 1 |
| `--data-bits <5\|6\|7\|8>` | Data bits, default 8 |
| `--addr <id>` | Station. Default 0. Same as `--modbus_address` |
| `--response-timeout <SECONDS>` | Wait for the connection and for each response. Default 5. Separate from `--timeout` |
| `--retries <COUNT>` | Extra attempts after a timeout or a broken connection. Default 2 |
| `--verbose`, `-v` | Print each transmitted and received frame, prefixed `tx` and `rx` |
| `--notation <pdu\|modicon>` | Address column. `pdu` is the default. `modicon` prints `00001`, `10001`, `30001`, and `40001`. Offsets above 9999 use six digits, so PDU 9999 is `410000` |

### Retries

The default `--retries 2` is the first try plus two more, three tries in all, then the command stops. `0` sends the request once. There is no extra wait between attempts. Each attempt still stops after `--response-timeout` seconds. The message from the last failed attempt is printed once.

A Modbus exception, such as `IllegalDataAddress`, is the device's answer and is not sent again. Station 0 is not retried, because that address gets no response and a repeated write would store the value again.

A repeating `--timeout` scan retries inside each sample. When those attempts are used up, the failed sample stays on screen and the scan continues. Opening the connection is a new attempt each time. A request retry uses the connection that is already open. If that socket closes after the request has been sent, the remaining attempts fail immediately. A write whose response was lost may already be stored. A retry sends that write again.

### Reads

```bash
mb_client read <area> --unit <register> [--addr <station>] [--qty <quantity>] [--timeout <SECONDS>] [--interval <MILLISECONDS>] [--log <FILE>] [--notation pdu|modicon] [--response-timeout <SECONDS>] [--retries <COUNT>] [--decode <format>...] [--order msrf|lsrf] [--swap-bytes] [connection options]
```

`--qty` defaults to 1. Coils and discrete inputs accept 1-2000. Holding and input registers accept 1-125. `0`, a number above that limit, or a non-decimal quantity is rejected before a connection is opened. The message names the range, for example `quantity 2001 is outside 1-2000 (FC 01/02)`.

`--timeout` is how long to keep reading. Omit it and the command reads once. A positive number repeats until that many seconds have passed. `0` or a negative number repeats until interrupted. On a terminal each sample replaces the previous one. A pipe or a file keeps every sample. Samples are `--interval` milliseconds apart (default 1000). `0` sends the next request as soon as the previous one finishes. A failed sample stays on screen. Each repeating sample ends with `requests=N valid=M`.

`--log FILE` appends one line per sample: a UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`, then the sample text with line breaks replaced by ` | `.

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

Other reads:

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

`read fifo` uses `--unit` as the pointer. That holding register holds the queue count, and the registers after it are the queued values. The count is at most 31. A larger count returns exception 03 (`IllegalDataValue`). The read does not clear the queue.

`read file` repeats `--group FILE:RECORD:LENGTH`. The file number is 1-65535. The record is 0-9999 (`0x270F`), and the length must stay inside that range. `1:10000:1` is rejected before a connection is opened.

`--code` is `basic`, `regular`, `extended`, or `specific`. It defaults to `basic`. `--object` defaults to 0. For `specific` it selects one object, 0-255. For the other codes it is the first object of the stream. An unknown stream object is answered as object 0. An unknown `specific` object returns exception 02. The memory server reports conformity `0x82`. An `extended` request is answered at that level, objects 0x00-0x06.

| Id | Name | Value |
| --- | --- | --- |
| 0x00 | VendorName | `mb-rs` |
| 0x01 | ProductCode | `mb` |
| 0x02 | MajorMinorRevision | the crate version |
| 0x03 | VendorUrl | `https://github.com/alfred-hong/mb-rs` |
| 0x04 | ProductName | `mb-rs` |
| 0x05 | ModelName | `memory-server` |
| 0x06 | UserApplicationName | empty |

`basic` returns 0x00-0x02. `regular` and `extended` return 0x00-0x06. CANopen general reference (FC 43/13) is not implemented.

### Decode

`--decode`, `--order`, and `--swap-bytes` apply to `holding`, `input`, `fifo`, `file`, and `read-write`. Coil and discrete reads do not take them. Without `--decode` the table shows decimal words, and `--verbose` adds a hex column. With `--decode` that place shows the interpretation. Repeat `--decode` to print one table per format.

`--swap-bytes` swaps the two bytes inside every 16-bit register before decoding. `--order` changes register order only for values that use more than one register. `--swap-bytes` and `--order lsrf` require at least one `--decode`.

| `--order` | Meaning |
| --- | --- |
| `msrf` | Most significant register first. Default. |
| `lsrf` | Least significant register first. |

| `--decode` | Meaning | Registers |
| --- | --- | --- |
| `bin` | binary | 1 |
| `hex` | hexadecimal | 1 |
| `ansi` | two characters, high byte first. Bytes `0x20`-`0x7E` and `0xA0`-`0xFF` are Latin-1. Every other byte is `.` | 1 |
| `u16` / `i16` | unsigned / signed 16-bit integer | 1 |
| `u32` / `i32` | unsigned / signed 32-bit integer | 2 |
| `u64` / `i64` | unsigned / signed 64-bit integer | 4 |
| `f32` / `f64` | IEEE 754 32-bit / 64-bit value. Very small and very large values use scientific notation | 2 / 4 |

`f32` and `f64` include the sign bit. There is no unsigned floating-point format. A trailing group that does not fill the format is reported and left undecoded.

```bash
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode i32
mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode i32 --decode f32
mb_client read input --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode u32 --order lsrf --swap-bytes
mb_client read fifo --ip 127.0.0.1 --addr 1 --unit 1 --decode i16
mb_client read file --ip 127.0.0.1 --addr 1 --group 4:7:2 --decode u32
```

### Writes

```bash
mb_client write <area> --unit <register> [--addr <station>] [--response-timeout <SECONDS>] [--retries <COUNT>] --value <values> [connection options]
```

One coil uses FC 05. `--value 0` is sent as `0x0000` (OFF). Any other word, such as `1` or `0x0002`, is sent as `0xFF00` (ON). Several coils use FC 15, 2-1968 comma-separated words, with the same 0 / non-zero rule. More than 1968 values is rejected before connecting.

On TCP, a function 05 frame whose value is not `0x0000` or `0xFF00` returns exception 03 and leaves the coil unchanged. `write coil` does not send those other values. `raw --function 5` rejects them before connecting, with `coil value must be FF00 or 0000`.

One holding register uses FC 06. Several use FC 16, at most 123 words.

Station 0 is broadcast. A write is stored and no response is sent, so the client times out. That address is not retried.

Mask write (FC 22) applies `(current AND --and) OR (--or AND NOT --and)`. Both masks are required. With the register holding `0x0012`:

```bash
mb_client write holding --ip 127.0.0.1 --addr 1 --unit 4 --value 0x0012
mb_client write mask --ip 127.0.0.1 --addr 1 --unit 4 --and 0x00F2 --or 0x0025
```

The register then reads `0x0017`.

File records (FC 21) use `--group FILE:RECORD:v1,v2`. The file number is 1-65535. Records run from 0 through 9999 (`0x270F`), including every value in the group. A record of 10000, or a span past 9999, is rejected before connecting.

```bash
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1
mb_client write coil --ip 192.168.1.100 --unit 0 --value 1,0,1,1
mb_client write holding --ip 192.168.1.100 --unit 100 --value 42,43,44
mb_client write file --ip 127.0.0.1 --addr 1 --group 4:7:0x1234,2
```

`read-write` (FC 23) writes first, then reads. `--value` is 1-121 words. `--read-qty` is 1-125 and defaults to 1. `--read-unit` is the first register to read. `--unit` is the first register to write. `--decode` prints only the values just read. The command runs once, because repeating it would write again.

```bash
mb_client read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

### Diagnostics

FC 08 is a serial-line function. The memory server also answers it on TCP.

`--sub` accepts a name or a number. `echo` is another name for `return-query`. For `ascii-delimiter`, the character is the high byte of `--data`, so `0x0A00` sets the delimiter to line feed. `listen-only` produces no response, so the client reports a timeout. `restart` with data `0xFF00` clears the counters and the event log.

```bash
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub return-query --data 0x1234
mb_client diagnostics --ip 127.0.0.1 --addr 1 --sub restart --data 0xFF00
```

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

### Raw function

`mb_client raw` sends one function code from 1 to 127. `--function` is decimal or `0x`. `--data` is the bytes after the function code, written as hex and separated by commas or spaces. An empty `--data` sends no extra bytes. `--data` does not apply the `--qty` limits. A raw read whose quantity is 0, whose register quantity is 126, or whose coil quantity is 2001 is sent, and the server returns exception 03. For `--function 5`, the value word must be `FF,00` or `00,00`.

```bash
mb_client raw --ip 127.0.0.1 --addr 1 --function 3 --data 00,00,00,01
```

## Server

`mb_server` is a memory server for Modbus TCP, RTU, and RTU over TCP. With no arguments it listens on `0.0.0.0:502` and answers station 1.

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

The server uses the same TCP and RTU flags as the client. `--timeout` is separate from the client scan window. The default is 5 seconds. It covers binding the TCP port, opening the serial port, and idle TCP connections.

Repeat `--modbus_address` or `--addr` to serve more than one station. Those stations share one memory image, and the list stays fixed for the process. Station 0 is the broadcast address and cannot be selected. A broadcast write updates the shared image and sends no response. A broadcast read gets no response. Any other id that was not listed gets no response.

The listener is chosen as follows:

- `--device` opens Modbus RTU on that serial port.
- `--rtu-over-tcp` serves RTU frames on TCP. `--ip` defaults to `0.0.0.0` and `--port` defaults to 502.
- `--ip`, or neither `--ip` nor `--device`, serves Modbus TCP. The default address is `0.0.0.0` port 502.
- `--device` together with `--ip` or `--rtu-over-tcp` is an error.

The client requires `--ip` or `--device`. Both together is an error.

Each memory count is 0-65535. The defaults are 10000.

- `--num-coils` starts each coil as a random 0 or 1.
- `--num-discrete` starts every discrete input OFF. Discrete inputs are not stored in the CSV file.
- `--num-holding` and `--num-input` start each register as a random integer 0-65535.

Each start without a restored snapshot draws a new set of those integers. Discrete inputs stay OFF. A client write to a coil or holding register replaces that value until another write. Input registers and discrete inputs stay read-only.

`--csv <FILE>` loads `area,address,value` rows before serving. The first non-empty line must be that header. Later empty lines are skipped. `area` is `coil`, `holding`, or `input`, case-insensitive on load and lowercase when the file is rewritten. `address` uses the same forms as the client. A register value is decimal digits only, 0-65535, with no `0x` prefix. A coil value is exactly `0` or `1`. The last row for an address wins. A clean stop, including Ctrl+C, rewrites the file with every address in those three areas. A forced kill does not rewrite it. If binding or opening the port fails, the file is left unchanged.

```csv
area,address,value
holding,0,1234
holding,40001,1
input,0,7
coil,0,1
```

An address outside the `--num-*` window returns exception 02. The CSV sets values inside that window. It does not remove addresses from it.

`--verbose` prints one line per request: `station=1 fc=0x03 addr=0 qty=4 result=ok from 127.0.0.1:54321`. `--delay-ms` waits after a normal request is stored and before the response is sent. `--exception-function` with `--exception-code` 2, 3, or 4 answers that function with the exception and does not change memory. Other functions are applied as usual. `--silence-writes` accepts a write, stores nothing, and sends no response.

The listen address, station list, snapshot path, delay, forced exception, and silence setting stay as they were at startup.

## Functions

Application behavior follows MODBUS Application Protocol Specification V1.1b3. Serial-line framing and the TCP MBAP header are specified in the companion implementation guides.

The client and the TCP memory server implement every public function code in V1.1b3 except 43/13. Quantity limits match the specification: coils and discrete inputs 1-2000, register reads 1-125, FC 15 at most 1968 coils, FC 16 at most 123 registers, and FC 23 writes at most 121 registers.

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

Functions 07, 11, 12, and 17 are marked serial-line only in the specification. This server also answers them on TCP.

These items stay out of this release, because no current library can add them without dropping function codes this project already implements. tokio-modbus 0.17.0 is the stack in use. rs-modbus 2.0 and rmodbus 0.12 can frame ASCII and UDP, but their servers cover functions 01-06, 15, and 16 (rs-modbus also 17, 22, 23, and 43/14). They do not cover 07, 08, 11, 12, 20, 21, and 24. rodbus does not add ASCII.

- Modbus ASCII
- Modbus UDP
- RS-485 RTS around each RTU response
- A configurable RTU inter-frame gap
- Receiving FC 08, FC 20, and FC 21 on the built-in RTU server and the RTU-over-TCP server. The RTU framer rejects those function codes while guessing the frame length. Modbus TCP receives them. A client can still send them to another RTU device.
- FC 43/13 CANopen general reference. V1.1b3 assigns MEI type 13 to CiA and does not publish the payload. The server returns exception 01.

Limits that remain with tokio-modbus 0.17.0:

- Report Server ID (FC 17) is sent as a custom `0x11` PDU. `Response::ReportServerId` sizes the buffer one byte shorter than the bytes the encoder writes.
- Bus communication-error, NAK, busy, and overrun counters stay 0. The codec drops a bad frame before it becomes a request, so those events are not visible to the server.
- Exception codes 05 (acknowledge), 06 (server device busy), 08 (memory parity), 0A (gateway path unavailable), and 0B (gateway target failed to respond) are not produced. This process is a memory server, not a programming port and not a gateway.

## Errors

- **Connection timeouts.** The message names the peer and how many seconds were allowed. After the retries are used up, that message is printed once.
- **Modbus exceptions.** The exception name, such as `IllegalDataAddress`, is printed once and the request is not sent again.
- **Validation.** A rejected quantity, address, file reference, station id, interval, coil value, or diagnostics name states the accepted range. `--qty` outside 1-2000 or 1-125, a file record outside 0-9999, more than 1968 coil values, more than 123 holding values, or a function 23 write outside 1-121 stops before a connection is opened. `raw --function 5` with a value other than `FF,00` or `00,00` stops with `coil value must be FF00 or 0000`. `--swap-bytes` and `--order lsrf` without `--decode` stop before a connection is opened. A negative number is reported as an invalid value.
- **Flag order.** `--ip`, `--device`, `--addr`, and the other connection flags follow the command. Using one of them before `read` or `write` says to put it after the command.
- **Read-only areas.** Writing discrete inputs or input registers stops before a connection is opened.
- **Area mismatch.** A `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` reference is rejected when its prefix belongs to another command.
- **Connection failures.** Network and serial-port errors include the system detail.

## Origin

- **Path:** https://github.com/wkrettek/mb-cli
- **Description:** A fast and reliable Modbus TCP and RTU client/server command-line tool. Built on top of [tokio-modbus](https://github.com/slowtec/tokio-modbus).
- **Thanks:** Warren Krettek, for the original `mb-cli` client, memory server, and the work this repository continues.
