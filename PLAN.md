# Modbus CLI Tool Development Plan

## Completed
- [x] Implement write operations for coils and holding registers
- [x] Add server subcommand framework
- [x] Implement the server functionality
    - [x] Create in-memory storage for all register types
    - [x] Implement Modbus TCP server using tokio-modbus
    - [x] Initialize values where each address equals its value
    - [x] Handle read/write requests from clients
    - [x] Manual verification of all read/write TCP functionality

- [x] Add support for Modbus RTU (serial) in addition to TCP
    - [x] Add serial port dependencies (tokio-serial)
    - [x] Add --device flag for serial port selection
    - [x] Implement RTU connection logic
    - [x] Add --baud flag for RTU baud rate configuration
    - [x] Handle exclusive access for virtual serial ports
- [x] Implement auto-detection for TCP vs RTU based on --ip or --device flags
    - [x] Make --ip and --device mutually exclusive
    - [x] Auto-select TCP when --ip is provided
    - [x] Auto-select RTU when --device is provided
    - [x] Manual verification of RTU read/write functionality

## Next: High Priority

## Critical Correctness Fixes: High Priority
- [x] **RTU serial port opening**: ~~Replace blocking `SerialStream::open()` with async version~~ (Won't do - current API is correct)
- [x] **RTU server unit ID**: ~~Fix hard-coded slave=1 in RTU server~~ (Won't do - no hard-coded slave found)
- [x] **Async-safe Mutex**: Replace `std::sync::Mutex` with `tokio::sync::RwLock` to prevent blocking async runtime
- [x] **Coil value parsing**: ~~Change from accepting any `u16` to proper `bool` parsing~~ (Won't do - being permissive is better)

## Usability Improvements: Medium Priority
- [x] **Range validation**: Add Clap range validation for qty (coils: 1-2000, registers: 1-125) with detailed Modbus spec error messages
- [x] **Verbose server logging**: ~~Gate server read/write logs behind `--verbose` flag~~ (Won't do - current output is fine)
- [x] **Serial port options**: Add optional `--parity`, `--stop-bits`, `--data-bits` flags for RS-485 compatibility

## Polish Improvements: Low Priority
- [x] **Table helper DRY**: Extract common header logic from print_register_table and print_coil_table
- [x] **Transport enum**: ~~Replace nested match with `enum Transport`~~ (Won't do - current tuple matching is clear and idiomatic)

## Before Publishing: Medium Priority
- [ ] Clean up cli output
    - [x] cli
    - [x] Hide more detail behind --verbose/-v
    - Maybe print a table of results?
    - [x] server
        - [x] print the action and from where (do later)
- [x] Lint and cleanup
    - [x] DRY violations:
        - [x] Extract common read operation error handling (4x duplication)
        - [x] Extract common write operation error handling (2x duplication) 
        - [x] Extract TCP server setup (2x duplication)
        - [x] Create error handling utilities (handle_modbus_response helper)
    - [x] Break into multiple files:
        - [x] cli.rs - CLI structs and parsing
        - [x] client.rs - Client connection and operations
            - [x] tcp
            - [x] rtu
        - [x] server.rs - Server implementation  
            - [x] tcp
            - [x] rtu
        - [x] table.rs - Table formatting
        - [x] main.rs - Just orchestration
    - [x] Remove dead code:
        - [x] Remove unused `format` field from Common struct
        - [x] Remove unused server variables (_unit, _verbose)
        - [x] Remove unused `arg` import from clap
        - [x] Fix help text examples (--bind → --ip)
    - [x] Add constants for magic numbers (ports, baud rates) - Won't do: code is clear as-is
    - [x] Get multiple AI reviews
        - [x] o3
        - [x] gemini
- [ ] Add comprehensive error handling and retry logic
    - [ ] Very good error messages for common issues
        - [x] inputs and discretes are not writeable
    - [x] Timeout configuration
- [x] Add basic unit tests for fast execution (28 tests total)
    - [x] CLI validation tests (6 tests)
        - [x] validate_coil_qty() with valid/invalid inputs
        - [x] validate_register_qty() with valid/invalid inputs
        - [x] Error message content validation
    - [x] Table formatting tests (8 tests)
        - [x] Empty input handling
        - [x] Single and multiple value tables
        - [x] Address calculation logic
        - [x] Header formatting
    - [x] Server logic tests (9 tests)
        - [x] ModbusData initialization
        - [x] Read operations (coils, holding registers)
        - [x] Write operations (single/multiple)
        - [x] Out-of-bounds exception handling
        - [x] Data persistence verification
    - [x] Client helper tests (5 tests)
        - [x] Timeout error handling
        - [x] Exception response handling
        - [x] Duration configuration
- [x] Add integration tests
    - [x] Basic integration tests (server on a free local port):
        - [x] Server startup and shutdown
        - [x] End-to-end CLI testing with `mb_client` against `mb_server`
        - [x] Network failure scenarios
        - [x] CLI argument validation
        - [x] Every public `mb_client` command against `mb_server`: four areas and aliases, address forms, decode formats, writes, mask, read-write, file, FIFO, diagnostics, identity, raw exceptions, and read-only writes
- [x] Create comprehensive documentation and examples
    - [x] README with usage examples
        - [x] Installation instructions
        - [x] Basic usage examples
        - [x] Command reference
        - [x] Protocol detection explanation
        - [x] Use case examples
- [ ] Package and publish
    - [ ] Cargo.toml metadata for publishing
    - [ ] Release preparation

## Future Enhancements
- [ ] Server configuration files: see "mb_server simulator behavior" (snapshot and sparse map)
- [ ] Probing functionality (removed from current scope)

## Upgrade dependencies
- [x] Bump direct crates to the latest crates.io releases
- [x] Adjust code for API changes without changing Modbus behavior
- [x] Update the documented Rust version to the new minimum
- [x] `cargo test` and `cargo clippy` pass

## Application functions and the four data areas
- [x] Accept decimal PDU addresses, `0x` hexadecimal PDU addresses, and 5- or 6-digit `0/1/3/4` area references
- [x] Add command aliases `discrete-output`/`0xxxx`, `discrete-input`/`1xxxx`, `input-register`/`3xxxx`, `holding-register`/`4xxxx`
- [x] Reject writes to discrete inputs and input registers before opening a connection
- [x] Implement functions 07, 08, 11, 12, 17, 20, 21, 22, 23, 24, and 43/14 on the client and memory server
- [x] Enforce write quantity limits and return exception 03 for an illegal quantity
- [x] Leave function 43/13 unimplemented; the RTU codec cannot frame functions 08, 20, and 21
- [x] Document the areas, address forms, and commands in the README

## Decode register values
- [x] Decode reads as binary, hexadecimal, ANSI, 16/32/64-bit signed and unsigned integers, and IEEE 754 `f32`/`f64`
- [x] Default 32/64-bit register order to MSRF and accept LSRF
- [x] Swap bytes inside each register for every decode format
- [x] Document the flags in the README
- [x] When `--decode` is set, print only the decoded table
- [x] README states that `--decode` replaces the raw register table

## Read duration
- [x] Client `--timeout` is seconds of repeated reading: omit for one read, a positive value stops at that time, `0` or negative repeats until interrupted
- [x] Server `--timeout` still covers bind, serial open, and idle TCP connections
- [x] Document the flag in the README
- [x] Repeating reads redraw the sample in place
- [x] Each sample is a returned string painted with cursor-up erase, not appended scrollback
- [x] README examples cover station flags, area addresses, every decode format, and live reads

## Align documentation with runtime
- [x] Install the `mb_client` and `mb_server` binaries from the `mb-cli` package
- [x] Server requires `--ip` or `--device`
- [x] Server `--unit` answers only that id
- [x] Server `--verbose` / `-v` logs each request and the TCP client address
- [x] Server `--timeout` covers bind, serial open, and idle TCP connections
- [x] Startup text matches coil/discrete OFF and register address initialization
- [x] CLI examples use the singular `coil` command

## Split the client and the server
- [x] `mb_client` contains only the client commands
- [x] `mb_server` is the memory server, with no client subcommands
- [x] Address parsing and serial settings stay in the shared library

## mb_server simulator behavior

A lab slave. These items add the behaviors common Modbus simulators expose. Named points, scripts, a web UI, and DF1 stay out. Sine animation is not included.

Decisions recorded here:

- The snapshot is one CSV. Each data row is `area,address,value`, the layout ModRSsim2 uses for type, number, and value on one line. `area` is `coil`, `holding`, or `input`. `address` uses the client address parser. A register value is a decimal integer 0–65535. A coil value is `0` or `1`. The header row is required. A duplicated address keeps the last row.
- The CSV covers coils, holding registers, and input registers. Discrete inputs stay OFF and are not in the file.
- Startup values for those three areas are a new random draw each process start: registers are integers 0–65535, coils are 0 or 1. Integers are rational, and a register cannot store a fraction. A CSV row replaces the random value at that address. On exit the file is rewritten with every address in the three configured areas, so the next start restores this image instead of drawing again.
- No per-second increment, coil toggle, or new random draw after startup. A client write to a coil or holding register replaces that value, and the value then stays. Input registers never change after startup.
- Fault delay commits a normal write, then waits, then responds. Silence on a write does not change data and sends nothing. A forced exception targets one function code chosen at startup and uses 02, 03, or 04. That function code does not write. A later request with a function code that is not forced writes as usual.
- Every configured station reads and writes the same memory image. Station ids come from repeated `--modbus_address` (or `--addr`) and are fixed at startup. Omitting the flag serves station 1. An id that is not in the list gets no response. There is no runtime switch to turn a station off, and exception 10 is not used for that.
- With no `--device`, the server listens on TCP `0.0.0.0` port 502. This replaces the old rule that `--ip` or `--device` is required. `--ip` still selects another address. `--ip` together with `--device` is still an error.
- While the process is running, listen address, station list, snapshot file, and fault settings stay as they were at startup. A client may still write coils and holding registers. Input registers and discrete inputs stay read-only.
- ASCII, UDP, RS-485 RTS-per-response, and RTU inter-frame gap tolerance are deferred. tokio-modbus 0.17.0 and tokio-serial 5.5.0 do not provide them. Do not implement them in this round.

### Step 1: Request log
- [x] With `--verbose`, log the function code, station id, start address, quantity, and outcome (normal response or exception code)
- [x] Keep the TCP peer address in that line

### Step 2: Snapshot
- [x] Load coils, holding registers, and input registers from `--csv <FILE>` before serving
- [x] Let a CSV row override the startup random value at that address
- [x] On exit, rewrite the CSV with every address in the three configured areas
- [x] Do not reload the CSV while the server is running
- [x] Reject a row whose address is outside the configured `--num-*` size, or whose area is `discrete`

### Step 3: Startup values
- [x] At startup, draw a new random integer 0–65535 for every holding and input register, and 0 or 1 for every coil
- [x] Leave discrete inputs OFF
- [x] After startup, leave input registers unchanged
- [x] After a client write to a coil or holding register, leave that value unchanged until another client write

### Step 4: Delay and forced faults
- [x] Optional fixed response delay, in milliseconds, after a normal write is stored and before the response is sent
- [x] Optional forced exception 02, 03, or 04 for one function code chosen at startup
- [x] Do not modify memory for that forced function code
- [x] Apply a later request when its function code is not the forced one
- [x] Optional silence on write requests: the write is not applied and no response is sent

### Step 5: Broadcast station 0
- [x] A write to station 0 updates the shared memory and produces no response
- [x] A read to station 0 produces no response
- [x] Station 0 cannot be selected as a normal station id

### Step 6: Listen address
- [x] Omit `--ip` and `--device` to bind TCP `0.0.0.0:502`
- [x] Document the default in the README, including the old "requires `--ip` or `--device`" examples

### Step 7: Several stations
- [x] Accept one station with `--modbus_address 1`, or several with repeated `--modbus_address`
- [x] Fix that list at startup
- [x] All of those stations share one coil, discrete, input, and holding image
- [x] Send no response for a station id that is not in the list

### Step 8: More transports
- [x] RTU over TCP, using tokio-modbus `server::rtu_over_tcp` (enable feature `rtu-over-tcp-server`)

### Step 9: Address window
- [x] Keep one contiguous window per area, sized by `--num-*` and starting at 0
- [x] Return exception 02 for an address outside that window
- [x] Use the CSV for values inside the window, not as a list of holes

### Deferred until a library can do them
Do not implement these in this round.

- [ ] Modbus ASCII
- [ ] Modbus UDP
- [ ] RS-485 RTS around each RTU response
- [ ] Configurable RTU inter-frame gap tolerance
- [ ] Functions 08, 20, and 21 on serial RTU and on RTU over TCP. `get_request_pdu_len` rejects 0x08, 0x14, and 0x15. RTU over TCP uses that same codec.

### Notes
Library check against tokio-modbus 0.17.0 and tokio-serial 5.5.0. The service future is awaited before a TCP or RTU response is sent, and `Ok(None)` skips the response. `ExceptionCode` includes 02, 03, and 04.

The earlier open points are closed. Per-second increment and coil toggle are not part of this server: they would change a writable value after it had been written. Unlisted CSV addresses are served inside the `--num-*` window with a startup random value; they are not holes that return exception 02.

A later check of rs-modbus 2.0, rmodbus 0.12, rodbus 1.5, and rusty-modbus-codec found no drop-in replacement. rs-modbus has ASCII and UDP, but not functions 07, 08, 11, 12, 20, 21, or 24. rmodbus frames ASCII and UDP only for functions 01–06, 15, and 16, and its length guess treats every other code as a fixed-size PDU. tokio-modbus 0.17.0 remains the newest release and still keeps `get_request_pdu_len` private. Do not switch stacks to gain ASCII or UDP.

## mb_client as a command-line ModScan

`mb_client` covers ModScan's master functions through flags. Windows UI, colors, fonts, charts, modem dialing, OLE, and database capture stay out.

- [x] Scan interval in milliseconds (`--interval`, default 1000, `0` means no extra wait)
- [x] A failed sample during `--timeout` stays on screen and the scan continues
- [x] Request and valid-reply counters on a repeating scan
- [x] One log line per sample (`--log`)
- [x] Connection and response wait separate from the scan window (`--response-timeout`, default 5 seconds)
- [x] `--verbose` prints transmitted and received bytes
- [x] `--notation modicon` prints 00001 / 10001 / 30001 / 40001, and six digits past offset 9999
- [x] One raw function code, ModScan's user command (`mb_client raw`)
- [ ] ModScan test script: name, station, function, address, length, data, and an expected result of data, exception 01/02/04, or no response. Deliberately bad CRC stays out
- [ ] ASCII, UDP, and RS-485 RTS timing stay deferred with the server. The current library does not provide them

