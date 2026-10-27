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
- [ ] Add integration tests (future enhancement)
    - [ ] Basic integration tests (server on port 5020):
        - [ ] Server startup and shutdown
        - [ ] End-to-end CLI testing
        - [ ] Network failure scenarios
        - [ ] CLI argument validation
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
- [ ] Server configuration files (holding file, etc.)
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
- [x] Install the `mb` binary from the `mb-cli` package
- [x] Server requires `--ip` or `--device`
- [x] Server `--unit` answers only that id
- [x] Server `--verbose` / `-v` logs each request and the TCP client address
- [x] Server `--timeout` covers bind, serial open, and idle TCP connections
- [x] Startup text matches coil/discrete OFF and register address initialization
- [x] CLI examples use the singular `coil` command
