use std::sync::Arc;
use tokio_modbus::client::{Reader, Writer};

mod actions;
mod address;
mod cli;
mod client;
mod decode;
mod handle;
mod output;
mod server;
mod table;

use actions::{ensure_write_coil_count, ensure_write_register_count, reject_readonly};
use cli::{Cli, Command, ReadArea, WriteArea};
use client::{connect_to_modbus, each_read, modbus_operation_with_timeout, REQUEST_TIMEOUT_SECS};
use server::{
    run_rtu_server, run_tcp_server, server_listen, ModbusData, ServerListen, ServerRuntime,
};
use table::{print_coil_table, print_register_table, write_coil_table};

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Read { area } => match area {
            ReadArea::Coil {
                start,
                qty,
                common,
                timing,
            } => {
                let mut client = connect_to_modbus(&common).await?;
                each_read(timing.timeout, async || {
                    let coils = modbus_operation_with_timeout(
                        || client.read_coils(start, qty),
                        "read coils",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} coil(s) (Modbus address: {}):\n",
                        coils.len(),
                        common.unit
                    );
                    write_coil_table(&mut frame, &coils, start);
                    Ok(frame)
                })
                .await?;
            }
            ReadArea::Discrete {
                start,
                qty,
                common,
                timing,
            } => {
                let mut client = connect_to_modbus(&common).await?;
                each_read(timing.timeout, async || {
                    let inputs = modbus_operation_with_timeout(
                        || client.read_discrete_inputs(start, qty),
                        "read discrete inputs",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} discrete input(s) (Modbus address: {}):\n",
                        inputs.len(),
                        common.unit
                    );
                    write_coil_table(&mut frame, &inputs, start);
                    Ok(frame)
                })
                .await?;
            }
            ReadArea::Holding {
                start,
                qty,
                decode,
                common,
                timing,
            } => {
                let mut client = connect_to_modbus(&common).await?;
                each_read(timing.timeout, async || {
                    let registers = modbus_operation_with_timeout(
                        || client.read_holding_registers(start, qty),
                        "read holding registers",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} holding register(s) (Modbus address: {}):\n",
                        registers.len(),
                        common.unit
                    );
                    decode::write_words(&mut frame, &registers, start, common.verbose, &decode)?;
                    Ok(frame)
                })
                .await?;
            }
            ReadArea::Input {
                start,
                qty,
                decode,
                common,
                timing,
            } => {
                let mut client = connect_to_modbus(&common).await?;
                each_read(timing.timeout, async || {
                    let registers = modbus_operation_with_timeout(
                        || client.read_input_registers(start, qty),
                        "read input registers",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} input register(s) (Modbus address: {}):\n",
                        registers.len(),
                        common.unit
                    );
                    decode::write_words(&mut frame, &registers, start, common.verbose, &decode)?;
                    Ok(frame)
                })
                .await?;
            }
            ReadArea::ExceptionStatus { common, timing } => {
                actions::read_exception_status(&common, timing.timeout).await?
            }
            ReadArea::EventCounter { common, timing } => {
                actions::read_event_counter(&common, timing.timeout).await?
            }
            ReadArea::EventLog { common, timing } => {
                actions::read_event_log(&common, timing.timeout).await?
            }
            ReadArea::ServerId { common, timing } => {
                actions::read_server_id(&common, timing.timeout).await?
            }
            ReadArea::Fifo {
                start,
                decode,
                common,
                timing,
            } => actions::read_fifo(&common, start, &decode, timing.timeout).await?,
            ReadArea::File {
                groups,
                decode,
                common,
                timing,
            } => actions::read_file(&common, &groups, &decode, timing.timeout).await?,
            ReadArea::DeviceId {
                code,
                object,
                common,
                timing,
            } => actions::read_device_id(&common, code, object, timing.timeout).await?,
        },

        Command::Write { area } => match area {
            WriteArea::Coil {
                start,
                values,
                common,
            } => {
                ensure_write_coil_count(values.len())?;
                let mut client = connect_to_modbus(&common).await?;

                // Convert u16 values to bool values (0 = false, non-zero = true)
                let bool_values: Vec<bool> = values.iter().map(|&v| v != 0).collect();

                if bool_values.len() == 1 {
                    // Single coil write (FC 5)
                    modbus_operation_with_timeout(
                        || client.write_single_coil(start, bool_values[0]),
                        "write coil",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    println!(
                        "Wrote coil at address {start} with value {} (Modbus address: {})",
                        if bool_values[0] { "ON" } else { "OFF" },
                        common.unit
                    );
                } else {
                    // Multiple coils write (FC 15)
                    modbus_operation_with_timeout(
                        || client.write_multiple_coils(start, &bool_values),
                        "write coils",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    println!(
                        "Wrote {} coil(s) starting at address {} (Modbus address: {})",
                        bool_values.len(),
                        start,
                        common.unit
                    );
                    print_coil_table(&bool_values, start);
                }
            }
            WriteArea::Holding {
                start,
                values,
                common,
            } => {
                ensure_write_register_count(values.len())?;
                let mut client = connect_to_modbus(&common).await?;

                if values.len() == 1 {
                    // Single register write (FC 6)
                    modbus_operation_with_timeout(
                        || client.write_single_register(start, values[0]),
                        "write register",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    if common.verbose {
                        println!(
                            "Wrote holding register at address {} with value {} (0x{:04X}) (Modbus address: {})",
                            start, values[0], values[0], common.unit
                        );
                    } else {
                        println!(
                            "Wrote holding register at address {} with value {} (Modbus address: {})",
                            start, values[0], common.unit
                        );
                    }
                } else {
                    // Multiple registers write (FC 16)
                    modbus_operation_with_timeout(
                        || client.write_multiple_registers(start, &values),
                        "write registers",
                        REQUEST_TIMEOUT_SECS,
                    )
                    .await?;
                    println!(
                        "Wrote {} holding register(s) starting at address {} (Modbus address: {})",
                        values.len(),
                        start,
                        common.unit
                    );
                    print_register_table(&values, start, common.verbose);
                }
            }
            WriteArea::Mask {
                start,
                and_mask,
                or_mask,
                common,
            } => actions::write_mask(&common, start, and_mask, or_mask).await?,
            WriteArea::File { groups, common } => actions::write_file(&common, &groups).await?,
            WriteArea::Discrete { .. } => {
                return Err(reject_readonly(
                    "Discrete inputs (1XXXX)",
                    "mb read discrete",
                ));
            }
            WriteArea::Input { .. } => {
                return Err(reject_readonly("Input registers (3XXXX)", "mb read input"));
            }
        },

        Command::Diagnostics { sub, data, common } => {
            actions::diagnostics(&common, sub, data).await?;
        }
        Command::ReadWrite {
            read_addr,
            read_qty,
            start,
            values,
            decode,
            common,
        } => actions::read_write(&common, read_addr, read_qty, start, &values, &decode).await?,

        Command::Server {
            ip,
            device,
            port,
            baud,
            parity,
            stop_bits,
            data_bits,
            unit,
            timeout,
            num_coils,
            num_discrete,
            num_holding,
            num_input,
            verbose,
        } => {
            let listen = server_listen(ip, device)?;
            let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(
                num_coils,
                num_discrete,
                num_holding,
                num_input,
            )));

            let print_config = || {
                println!("Configuration:");
                println!("  Modbus address: {unit}");
                println!("  Timeout: {timeout}s");
                println!(
                    "  Coils: {} (addresses 0-{}), initial value OFF",
                    num_coils,
                    num_coils.saturating_sub(1)
                );
                println!(
                    "  Discrete Inputs: {} (addresses 0-{}), initial value OFF",
                    num_discrete,
                    num_discrete.saturating_sub(1)
                );
                println!(
                    "  Holding Registers: {} (addresses 0-{}), initial value equals the address",
                    num_holding,
                    num_holding.saturating_sub(1)
                );
                println!(
                    "  Input Registers: {} (addresses 0-{}), initial value equals the address",
                    num_input,
                    num_input.saturating_sub(1)
                );
                println!();
            };

            let runtime = ServerRuntime {
                unit,
                verbose,
                timeout_secs: timeout,
            };
            match listen {
                ServerListen::Tcp(ip_addr) => {
                    println!("Starting Modbus TCP server on {ip_addr}:{port}");
                    print_config();
                    run_tcp_server(ip_addr, port, runtime, data).await?;
                }
                ServerListen::Rtu(device_path) => {
                    println!("Starting Modbus RTU server on {}", device_path.display());
                    print_config();
                    run_rtu_server(
                        &device_path,
                        baud,
                        &parity,
                        &stop_bits,
                        &data_bits,
                        runtime,
                        data,
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}
