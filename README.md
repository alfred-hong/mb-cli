# mb-cli

A fast and reliable Modbus TCP and RTU client/server command-line tool. Built on top of [tokio-modbus](https://github.com/slowtec/tokio-modbus).

The definitive hub for all standard specifications is the Modbus Organization, an independent group of users and suppliers that governs the protocol.
• Primary Web Portal: www.modbus.org
• [Documentation](https://www.modbus.org/modbus-specifications)

## Features

- **Modbus TCP** and **RTU** support with automatic protocol detection
- **Client operations**: The public Modbus application functions for the four data areas, plus diagnostics, file records, FIFO, mask write, and device identification
- **Server mode**: Run a Modbus TCP or RTU memory server that answers those functions
- **Addresses**: Decimal PDU addresses, `0x` hexadecimal PDU addresses, and classic `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` references
- **Decoding**: `--decode` replaces the raw register table. Formats are binary, hexadecimal, ANSI, 16/32/64-bit integers, and IEEE 754 `f32`/`f64`, with byte swap and MSRF/LSRF register order
- **Live read**: `--timeout` repeats a read and redraws that sample in place. A positive number is the length in seconds. `0` or a negative number continues until interrupted
- **Table output**: One address/value table. Without `--decode` the values are decimal words. With `--decode` the values are that interpretation

## Installation

```bash
cargo install --locked mb-cli
```

The package name is `mb-cli`. The command it installs is `mb`.

### From Source

```bash
# Clone the repository
git clone <repository-url>
cd mb-cli

# Build and install
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
mb read holding --ip 192.168.1.100 --unit 0 --qty 10

# Same read, naming the station with either flag
mb read holding --ip 192.168.1.100 --modbus_address 1 --unit 0 --qty 10
mb read holding --ip 192.168.1.100 --addr 1 --unit 0 --qty 10

# Read the other three areas
mb read coil --ip 192.168.1.100 --addr 1 --unit 00001 --qty 8
mb read discrete --ip 192.168.1.100 --addr 1 --unit 10001 --qty 8
mb read input --ip 192.168.1.100 --addr 1 --unit 30001 --qty 4

# Read coils from an RTU device
mb read coil --device /dev/ttyUSB0 --addr 1 --unit 0 --qty 8 --baud 9600

# Read with verbose output. The extra column is hexadecimal.
mb read holding --ip 127.0.0.1 --addr 1 --unit 100 --qty 5 --verbose

# With --decode, the table is the interpretation.
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode i32

# Keep reading for 10 seconds. Each sample redraws in place, then the command exits.
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 20 --timeout 10

# Decode while the table redraws in place
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --decode bin

# Keep reading until interrupted. 0 and a negative value do the same thing.
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --timeout 0
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --timeout -1
```

#### Write Operations

```bash
# Write single coil
mb write coil --ip 192.168.1.100 --unit 0 --value 1

# Write multiple coils
mb write coil --ip 192.168.1.100 --unit 0 --value 1,0,1,1

# Write holding register
mb write holding --ip 192.168.1.100 --unit 100 --value 42

# Write multiple registers
mb write holding --ip 192.168.1.100 --unit 100 --value 42,43,44
```

#### Server Mode

```bash
# Start TCP server. Station defaults to 1.
mb server --ip 0.0.0.0 --port 502

# Answer only station 7. --addr is the same flag.
mb server --ip 0.0.0.0 --port 502 --modbus_address 7

# Idle TCP connections close after 12 seconds
mb server --ip 127.0.0.1 --port 5020 --timeout 12 --verbose

# Start RTU server
mb server --device /dev/ttyUSB0 --baud 9600 --parity even --stop-bits 1 --data-bits 8 --addr 1

# Server with a custom memory layout
mb server --ip 0.0.0.0 --num-coils 1000 --num-discrete 1000 --num-holding 500 --num-input 500
```

### Command Reference

#### Global Options

- `--timeout <SECONDS>` on a **read** command is how long to keep reading. Omit it and the command reads once. `10` repeats the read for 10 seconds, then exits. `0` or a negative value repeats until you interrupt it. On a terminal each sample replaces the previous one in the same place, one second apart. Output sent to a pipe or a file keeps every sample. Connecting, and each individual request, still stops after 5 seconds when the peer stays silent.
- Server `--timeout <seconds>` is separate (default: 5). It covers binding the TCP port, opening the serial port, and idle TCP connections.
- `--verbose` / `-v` - Enable verbose output. On the server this prints each request and, for TCP, the client address.
- `--modbus_address <id>` - Modbus station address (default: 0 for the client, 1 for the server). `--addr` is the same flag. The server answers only this id and stays silent for any other id.

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

`mb write discrete` and `mb write input` stop immediately. Those tables are read-only.

#### Addresses and values

`--unit` accepts three forms:

- Decimal PDU address: `0`, `100`, `40000`. A number that is not a 5- or 6-digit area reference is the 0-based PDU address.
- Hexadecimal PDU address: `0x64`, `0X00FF`.
- Area reference. Five digits use an offset of 1-9999 (`00001`, `10001`, `30001`, `40001`, `40100`). Six digits use an offset of 1-65536 (`400001`, `465536`). The PDU address is the offset minus 1. A reference whose prefix belongs to another area is rejected. An offset of 0, such as `40000`, is not a reference and stays a decimal PDU address.

Holding PDU 0 can be written `0`, `0x0`, `0x00`, `40001`, or `400001`. Holding PDU 1 can be written `1`, `0x1`, `0x01`, `40002`, or `400002`. `0x1` and `0x01` are the same hexadecimal PDU address. A classic reference uses its own area prefix: `00001` is coil PDU 0, `10001` is discrete PDU 0, and `30001` is input PDU 0.

`--value`, `--and`, `--or`, and `--data` accept decimal or `0x` hexadecimal words. Quantities stay decimal.

```bash
mb read holding --ip 127.0.0.1 --addr 1 --unit 40001
mb read holding --ip 127.0.0.1 --addr 1 --unit 400001
mb read holding --ip 127.0.0.1 --addr 1 --unit 0x01
mb read holding --ip 127.0.0.1 --addr 1 --unit 40100
mb read coil --ip 127.0.0.1 --addr 1 --unit 00001
mb read coil --ip 127.0.0.1 --addr 1 --unit 0x10 --qty 8
mb read 0xxxx --ip 127.0.0.1 --addr 1 --unit 0 --qty 8
mb read discrete-input --ip 127.0.0.1 --addr 1 --unit 10001
mb read input-register --ip 127.0.0.1 --addr 1 --unit 30001
mb read holding-register --ip 127.0.0.1 --addr 1 --unit 40002
mb write holding --ip 127.0.0.1 --addr 1 --unit 100 --value 0x2A
```

`--unit` is the register address. `--modbus_address` and `--addr` are the station address. The built-in server answers station 1 unless that flag changes it. Client commands use station 0 unless you pass `--addr 1`.

#### Decoding register values

These flags are accepted by `mb read holding`, `mb read input`, `mb read fifo`, `mb read file`, and `mb read-write`. The count line stays. Without `--decode` the table shows decimal words, and `--verbose` adds a hex column there. With `--decode` that place shows the requested interpretation. Repeat `--decode` to print one decoded table per format.

`--swap-bytes` swaps the two bytes inside every 16-bit register before that interpretation. It applies to every format in the table. `--order` changes register order only for values that use more than one register.

| `--order` | Meaning |
| --- | --- |
| `msrf` | Most significant register first. The first register holds the high bits. This is the default. |
| `lsrf` | Least significant register first. The registers are reversed. |

| `--decode` | Meaning | Registers |
| --- | --- | --- |
| `bin` | binary (二进制) | 1 |
| `hex` | hexadecimal (十六进制) | 1 |
| `ansi` | two ANSI characters, high byte first (ANSI 字符) | 1 |
| `u16` / `i16` | unsigned / signed 16-bit integer (无符号 / 有符号整数) | 1 |
| `u32` / `i32` | unsigned / signed 32-bit integer | 2 |
| `u64` / `i64` | unsigned / signed 64-bit integer | 4 |
| `f32` / `f64` | IEEE 754 32-bit / 64-bit decimal (小数) | 2 / 4 |

`f32` and `f64` are IEEE 754 values, so the sign bit is part of the encoding. There is no separate unsigned 32-bit or 64-bit floating-point format. A trailing group that does not fill the format is reported and left undecoded. `--swap-bytes` and `--order lsrf` require at least one `--decode`.

```bash
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode bin
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode hex
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode u16
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 1 --decode i16
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode i32 --decode f32
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --decode u64 --decode i64 --decode f64
mb read input --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode u32 --order lsrf --swap-bytes
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 2 --decode ansi --swap-bytes
mb read holding --ip 127.0.0.1 --addr 1 --unit 0 --qty 4 --timeout 10 --decode bin
mb read fifo --ip 127.0.0.1 --addr 1 --unit 1 --decode i16
mb read file --ip 127.0.0.1 --addr 1 --group 4:7:2 --decode u32
mb read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

#### Read commands

```bash
mb read <area> --unit <register> [--modbus_address <station>] [--qty <quantity>] [--timeout <SECONDS>] [--decode <format>...] [--order msrf|lsrf] [--swap-bytes] [connection options]
```

- `--qty` defaults to 1. Coils and discrete inputs (FC 01/02) accept 1-2000. Holding and input registers (FC 03/04) accept 1-125.
- `--decode`, `--order`, and `--swap-bytes` apply to `holding` and `input`. Coil and discrete reads do not take them. FIFO, file, and `read-write` accept the same three flags. A decoded read prints the interpretation in place of the raw register table.
- `--timeout` repeats that read and redraws the sample in place. It is not accepted by writes. `mb read-write` still runs once, because repeating it would write again.

Other reads:

```bash
mb read exception-status --ip 127.0.0.1 --addr 1
mb read event-counter --ip 127.0.0.1 --addr 1
mb read event-log --ip 127.0.0.1 --addr 1
mb read server-id --ip 127.0.0.1 --addr 1
mb read fifo --ip 127.0.0.1 --addr 1 --unit 1
mb read file --ip 127.0.0.1 --addr 1 --group 4:1:2 --group 3:9:2
mb read device-id --ip 127.0.0.1 --addr 1 --code basic
mb read device-id --ip 127.0.0.1 --addr 1 --code regular
mb read device-id --ip 127.0.0.1 --addr 1 --code extended
mb read device-id --ip 127.0.0.1 --addr 1 --code specific --object 0
```

`--code` is `basic`, `regular`, `extended`, or `specific`. `--object` selects one object for `specific`, or the first object of a stream. The memory server reports conformity `0x82` (regular identification, stream and individual access). An extended request is answered at that level. CANopen general reference (FC 43/13) is not implemented.

Objects returned by this server:

| Id | Name | Value |
| --- | --- | --- |
| 0x00 | VendorName | `mb-cli` |
| 0x01 | ProductCode | `mb` |
| 0x02 | MajorMinorRevision | the crate version |
| 0x03 | VendorUrl | `https://github.com/wkrettek/mb-cli` |
| 0x04 | ProductName | `mb-cli` |
| 0x05 | ModelName | `memory-server` |
| 0x06 | UserApplicationName | empty |

`basic` returns 0x00–0x02. `regular` and `extended` return 0x00–0x06.

#### Write commands

```bash
mb write <area> --unit <register> [--modbus_address <station>] --value <values> [connection options]
```

- One coil uses FC 05. Several coils use FC 15, at most 1968 values. `0` is OFF and any other word is ON.
- One holding register uses FC 06. Several use FC 16, at most 123 values.
- Mask write (FC 22) applies `(current AND and-mask) OR (or-mask AND NOT and-mask)`:

```bash
mb write mask --ip 127.0.0.1 --addr 1 --unit 4 --and 0x00F2 --or 0x0025
```

- File records (FC 21). Each group is `FILE:RECORD:v1,v2`:

```bash
mb write file --ip 127.0.0.1 --addr 1 --group 4:7:0x1234,2
```

- Read/write multiple registers (FC 23) writes first, then reads. The write is 1-121 registers and the read is 1-125. `--decode` prints only the interpretation of the values just read:

```bash
mb read-write --ip 127.0.0.1 --addr 1 --read-unit 0 --read-qty 2 --unit 0 --value 9,8 --decode i32
```

#### Diagnostics

FC 08 is a serial-line function. The memory server also answers it on TCP.

```bash
mb diagnostics --ip 127.0.0.1 --addr 1 --sub return-query --data 0x1234
mb diagnostics --ip 127.0.0.1 --addr 1 --sub echo --data 0x1234
mb diagnostics --ip 127.0.0.1 --addr 1 --sub restart --data 0xFF00
mb diagnostics --ip 127.0.0.1 --addr 1 --sub ascii-delimiter --data 0x0A00
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

`listen-only` produces no response, so the client reports a timeout. `restart` with data `0xFF00` clears the counters and the event log. Functions 07, 11, 12, and 17 are serial-line functions as well; this server answers them on TCP.

The tokio-modbus RTU framer cannot size requests for FC 08, FC 20, and FC 21, so the built-in RTU server does not receive those three. TCP does. A client can still send them to another RTU device.

#### Function coverage

| Code | Command | Notes |
| --- | --- | --- |
| 01 | `mb read coil` | 1-2000 coils |
| 02 | `mb read discrete` | 1-2000 discrete inputs |
| 03 | `mb read holding` | 1-125 holding registers |
| 04 | `mb read input` | 1-125 input registers |
| 05 / 15 | `mb write coil` | one coil, or 2-1968 coils |
| 06 / 16 | `mb write holding` | one register, or 2-123 registers |
| 07 | `mb read exception-status` | first 8 coils, bit 0 is address 0 |
| 08 | `mb diagnostics` | serial-line diagnostics |
| 11 | `mb read event-counter` | status and event count |
| 12 | `mb read event-log` | newest event first, 64 retained |
| 17 | `mb read server-id` | station address, run status, `mb-cli` |
| 20 | `mb read file` | file 1-65535, records 0-0x270F, reference type 6 |
| 21 | `mb write file` | echoes the request |
| 22 | `mb write mask` | one holding register |
| 23 | `mb read-write` | write, then read |
| 24 | `mb read fifo` | count in the pointer register, at most 31 values |
| 43 / 14 | `mb read device-id` | MEI type 14 |
| 43 / 13 | — | CANopen general reference is not implemented |

A quantity of 0 or a quantity above the function limit returns exception 03 (illegal data value). An address outside the configured table returns exception 02 (illegal data address). An unknown function returns exception 01. Reserved and user-defined function codes are not implemented. File records start at 0 until written. The FIFO read does not clear the queue. Bus communication-error, NAK, busy, and overrun counters stay 0 because the codec drops frames that never become requests.

#### Server Command

```bash
mb server [connection options] [memory options]
```

The server accepts the same TCP and RTU options as the client, including `--parity`, `--stop-bits`, and `--data-bits`.

Memory options, each 0-65535:
- `--num-coils <count>` - Number of coils (default: 10000). Coils start OFF.
- `--num-discrete <count>` - Number of discrete inputs (default: 10000). Discrete inputs start OFF.
- `--num-holding <count>` - Number of holding registers (default: 10000). Each register starts equal to its address.
- `--num-input <count>` - Number of input registers (default: 10000). Each register starts equal to its address.

### Protocol Detection

The tool automatically detects the protocol based on arguments:

- `--ip` specified → Modbus TCP
- `--device` specified → Modbus RTU
- Both specified → Error (mutually exclusive)
- Neither specified → Error (must specify one)

### Examples by Use Case

#### Industrial Automation

```bash
# Read PLC outputs at station 1
mb read coil --ip 192.168.1.10 --addr 1 --unit 0 --qty 16

# Read sensor values
mb read input --ip 192.168.1.10 --addr 1 --unit 1000 --qty 4 --verbose

# Watch a holding block for 10 seconds
mb read holding --ip 192.168.1.10 --addr 1 --unit 0 --qty 4 --timeout 10 --decode i32

# Write setpoint
mb write holding --ip 192.168.1.10 --addr 1 --unit 2000 --value 1500
```

#### Serial Device Communication

```bash
# Read from RTU device with custom baud rate
mb read holding --device /dev/ttyUSB0 --baud 19200 --addr 1 --unit 0 --qty 10

# Write to RTU device
mb write coil --device /dev/ttyUSB0 --addr 2 --unit 0 --value 1
```

#### Testing and Development

```bash
# Start test server (Modbus address defaults to 1)
mb server --ip 127.0.0.1 --port 5020 &

# Test client against local server
mb read holding --ip 127.0.0.1 --port 5020 --addr 1 --unit 0 --qty 5

# Test with verbose logging
mb read coil --ip 127.0.0.1 --port 5020 --addr 1 --unit 0 --qty 8 --verbose
```

## Error Handling

The tool provides detailed error messages for common issues:

- **Connection timeouts**: Clear timeout messages with duration
- **Modbus exceptions**: Detailed exception codes (IllegalDataAddress, etc.)
- **Validation errors**: Specification-compliant range checking with explanations
- **Read-only areas**: Writing discrete inputs or input registers stops before a connection is opened
- **Area mismatch**: A `0XXXX` / `1XXXX` / `3XXXX` / `4XXXX` reference is rejected when its prefix belongs to another command
- **Connection failures**: Network and serial port error details
