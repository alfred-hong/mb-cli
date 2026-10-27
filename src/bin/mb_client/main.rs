use tokio_modbus::client::{Reader, Writer};

mod actions;
mod cli;
mod client;
mod decode;
mod output;
mod table;
mod trace;

use actions::{area_style, ensure_write_coil_count, ensure_write_register_count, reject_readonly};
use cli::{parse_hex_payload, Cli, Command, ReadArea, WriteArea};
use client::{connect_to_modbus, each_read, modbus_operation_with_timeout};
use decode::ensure_decode_flags;
use table::{print_coil_table, print_register_table, write_coil_table};

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let text = err.to_string();
            eprint!("{text}");
            if connection_flag_before_command(&text) {
                eprintln!(
                    "Put connection and station flags after the command, for example: mb_client read holding --ip 127.0.0.1 --addr 1 --unit 0"
                );
            }
            std::process::exit(err.exit_code());
        }
    };

    match cli.cmd {
        Command::Read { area } => match area {
            ReadArea::Coil {
                start,
                qty,
                common,
                timing,
            } => {
                let mut client = connect_to_modbus(&common).await?;
                each_read(&timing, async || {
                    let coils = modbus_operation_with_timeout(
                        || client.read_coils(start, qty),
                        "read coils",
                        common.response_timeout,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} coil(s) (Modbus address: {}):\n",
                        coils.len(),
                        common.unit
                    );
                    write_coil_table(&mut frame, &coils, start, area_style(common.notation, 0));
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
                each_read(&timing, async || {
                    let inputs = modbus_operation_with_timeout(
                        || client.read_discrete_inputs(start, qty),
                        "read discrete inputs",
                        common.response_timeout,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} discrete input(s) (Modbus address: {}):\n",
                        inputs.len(),
                        common.unit
                    );
                    write_coil_table(&mut frame, &inputs, start, area_style(common.notation, 1));
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
                ensure_decode_flags(&decode)?;
                let mut client = connect_to_modbus(&common).await?;
                each_read(&timing, async || {
                    let registers = modbus_operation_with_timeout(
                        || client.read_holding_registers(start, qty),
                        "read holding registers",
                        common.response_timeout,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} holding register(s) (Modbus address: {}):\n",
                        registers.len(),
                        common.unit
                    );
                    decode::write_words(
                        &mut frame,
                        &registers,
                        start,
                        common.verbose,
                        &decode,
                        area_style(common.notation, 4),
                    )?;
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
                ensure_decode_flags(&decode)?;
                let mut client = connect_to_modbus(&common).await?;
                each_read(&timing, async || {
                    let registers = modbus_operation_with_timeout(
                        || client.read_input_registers(start, qty),
                        "read input registers",
                        common.response_timeout,
                    )
                    .await?;
                    let mut frame = format!(
                        "Read {} input register(s) (Modbus address: {}):\n",
                        registers.len(),
                        common.unit
                    );
                    decode::write_words(
                        &mut frame,
                        &registers,
                        start,
                        common.verbose,
                        &decode,
                        area_style(common.notation, 3),
                    )?;
                    Ok(frame)
                })
                .await?;
            }
            ReadArea::ExceptionStatus { common, timing } => {
                actions::read_exception_status(&common, &timing).await?
            }
            ReadArea::EventCounter { common, timing } => {
                actions::read_event_counter(&common, &timing).await?
            }
            ReadArea::EventLog { common, timing } => {
                actions::read_event_log(&common, &timing).await?
            }
            ReadArea::ServerId { common, timing } => {
                actions::read_server_id(&common, &timing).await?
            }
            ReadArea::Fifo {
                start,
                decode,
                common,
                timing,
            } => actions::read_fifo(&common, start, &decode, &timing).await?,
            ReadArea::File {
                groups,
                decode,
                common,
                timing,
            } => actions::read_file(&common, &groups, &decode, &timing).await?,
            ReadArea::DeviceId {
                code,
                object,
                common,
                timing,
            } => actions::read_device_id(&common, code, object, &timing).await?,
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
                        common.response_timeout,
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
                        common.response_timeout,
                    )
                    .await?;
                    println!(
                        "Wrote {} coil(s) starting at address {} (Modbus address: {})",
                        bool_values.len(),
                        start,
                        common.unit
                    );
                    print_coil_table(&bool_values, start, area_style(common.notation, 0));
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
                        common.response_timeout,
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
                        common.response_timeout,
                    )
                    .await?;
                    println!(
                        "Wrote {} holding register(s) starting at address {} (Modbus address: {})",
                        values.len(),
                        start,
                        common.unit
                    );
                    print_register_table(
                        &values,
                        start,
                        common.verbose,
                        area_style(common.notation, 4),
                    );
                }
            }
            WriteArea::Mask {
                start,
                and_mask,
                or_mask,
                common,
            } => actions::write_mask(&common, start, and_mask, or_mask).await?,
            WriteArea::File { groups, common } => actions::write_file(&common, &groups).await?,
            WriteArea::Discrete {
                start,
                values,
                common: _,
            } => {
                let _ = (start, values);
                return Err(reject_readonly(
                    "Discrete inputs (1XXXX)",
                    "mb_client read discrete",
                ));
            }
            WriteArea::Input {
                start,
                values,
                common: _,
            } => {
                let _ = (start, values);
                return Err(reject_readonly(
                    "Input registers (3XXXX)",
                    "mb_client read input",
                ));
            }
        },

        Command::Diagnostics { sub, data, common } => {
            actions::diagnostics(&common, sub, data).await?;
        }
        Command::Raw {
            function,
            data,
            common,
        } => {
            let bytes = parse_hex_payload(&data)?;
            let response = actions::raw_request(&common, function, &bytes).await?;
            println!("{response:?}");
        }
        Command::ReadWrite {
            read_addr,
            read_qty,
            start,
            values,
            decode,
            common,
        } => actions::read_write(&common, read_addr, read_qty, start, &values, &decode).await?,
    }

    Ok(())
}

/// Clap reports a flag used before the subcommand as an unexpected argument.
fn connection_flag_before_command(text: &str) -> bool {
    if !text.contains("unexpected argument") {
        return false;
    }
    [
        "'--ip'",
        "'--device'",
        "'--port'",
        "'--baud'",
        "'--parity'",
        "'--stop-bits'",
        "'--data-bits'",
        "'--modbus_address'",
        "'--addr'",
        "'--verbose'",
        "'-v'",
        "'--notation'",
        "'--response-timeout'",
    ]
    .iter()
    .any(|flag| text.contains(flag))
}
