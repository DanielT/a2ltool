use std::ffi::OsStr;

use crate::ifdata::{
    A2mlVector, Address2, CAN_Parameters, Channel, Cmd, CycleRepetition, Daq2, EvServ,
    FLX_Parameters, FlxSlotId, HostName, InitialCmdBuffer, InitialResErrBuffer, Ipv6, LpduId,
    MaxFlxLenBuf, Offset, PoolBuffer, ResErr, Stim2, TCP_IP_Parameters, UDP_IP_Parameters, XCPplus,
    Xcp, XcpPacket,
};
use a2lfile::{A2lFile, A2lObject, A2lObjectName};

pub(crate) fn show_settings(a2l_file: &A2lFile, filename: &OsStr) {
    let multi_module = a2l_file.project.module.len() > 1;

    println!("XCP settings in {}:", filename.to_string_lossy());

    for module in &a2l_file.project.module {
        if multi_module {
            println!("XCP settings for module {}", module.get_name());
        }

        let mut found = false;
        for ifdata in &module.if_data {
            if !ifdata.ifdata_valid {
                println!(
                    "Warning: the IF_DATA block on line {} is not valid",
                    ifdata.get_layout().line
                );
            }
            if let Some(decoded_ifdata) = A2mlVector::load_from_ifdata(ifdata) {
                if let Some(xcp) = &decoded_ifdata.xcp {
                    print_xcp(xcp);
                    found = true;
                }
                if let Some(xcpplus) = &decoded_ifdata.xcpplus {
                    print_xcpplus(xcpplus);
                    found = true;
                }
            }
        }

        if !found {
            println!("No XCP settings found in module {}", module.get_name());
        }
    }
    println!();
}

fn print_xcp(xcp: &Xcp) {
    if let Some(xcp_on_can) = &xcp.xcp_on_can {
        print_xcp_on_can(&xcp_on_can.can_parameters);
    }

    if let Some(xcp_on_flx) = &xcp.xcp_on_flx {
        print_xcp_on_flx(&xcp_on_flx.flx_parameters);
    }

    if let Some(xcp_on_tcp_ip) = &xcp.xcp_on_tcp_ip {
        print_xcp_on_tcp_ip(&xcp_on_tcp_ip.tcp_ip_parameters);
    }

    if let Some(xcp_on_udp_ip) = &xcp.xcp_on_udp_ip {
        print_xcp_on_udp_ip(&xcp_on_udp_ip.udp_ip_parameters);
    }
}
fn print_xcpplus(xcpplus: &XCPplus) {
    for xcp_on_can in &xcpplus.xcp_on_can {
        print_xcp_on_can(&xcp_on_can.can_parameters);
    }

    for xcp_on_flx in &xcpplus.xcp_on_flx {
        print_xcp_on_flx(&xcp_on_flx.flx_parameters);
    }

    for xcp_on_tcp_ip in &xcpplus.xcp_on_tcp_ip {
        print_xcp_on_tcp_ip(&xcp_on_tcp_ip.tcp_ip_parameters);
    }

    for xcp_on_udp_ip in &xcpplus.xcp_on_udp_ip {
        print_xcp_on_udp_ip(&xcp_on_udp_ip.udp_ip_parameters);
    }
}

fn print_xcp_on_can(can_parameters: &CAN_Parameters) {
    println!("  XCP on CAN:");
    if let Some(can_id_master) = &can_parameters.can_id_master {
        println!(
            "    CAN id master: 0x{:X}",
            (can_id_master.value & 0x1fff_ffff)
        );
    }
    if let Some(can_id_slave) = &can_parameters.can_id_slave {
        println!(
            "    CAN id slave: 0x{:X}",
            (can_id_slave.value & 0x1fff_ffff)
        );
    }
    if let Some(baudrate) = &can_parameters.baudrate {
        println!("    CAN baudrate: {} kbps", baudrate.value / 1000);
    }
    if let Some(can_fd) = &can_parameters.can_fd {
        println!("    CAN-FD enabled:");
        if let Some(baudrate) = &can_fd.can_fd_data_transfer_baudrate {
            println!("      CAN-FD data baudrate: {} kbps", baudrate.value / 1000);
        }
        if let Some(max_dlc) = &can_fd.max_dlc {
            println!("      CAN-FD max DLC: {}", max_dlc.value);
        }
    }
}

fn print_xcp_on_flx(flx_parameters: &FLX_Parameters) {
    println!("  XCP on Flexray");
    if !flx_parameters.fibex_file.is_empty() {
        println!("    fibex file: {}", flx_parameters.fibex_file);
    }

    if let Some(buffer) = &flx_parameters.initial_cmd_buffer {
        let InitialCmdBuffer {
            flx_buf,
            max_flx_len_buf,
            lpdu_id,
            xcp_packet,
            ..
        } = buffer;
        println!("    Initial cmd buffer:");
        print_xcp_on_flx_buffer(*flx_buf, max_flx_len_buf, lpdu_id, xcp_packet);
    }

    if let Some(buffer) = &flx_parameters.initial_res_err_buffer {
        let InitialResErrBuffer {
            flx_buf,
            max_flx_len_buf,
            lpdu_id,
            xcp_packet,
            ..
        } = buffer;
        println!("    Initial res / err buffer:");
        print_xcp_on_flx_buffer(*flx_buf, max_flx_len_buf, lpdu_id, xcp_packet);
    }

    for buffer in &flx_parameters.pool_buffer {
        let PoolBuffer {
            flx_buf,
            max_flx_len_buf,
            lpdu_id,
            xcp_packet,
            ..
        } = buffer;
        println!("    pool buffer:");
        print_xcp_on_flx_buffer(*flx_buf, max_flx_len_buf, lpdu_id, xcp_packet);
    }
}

fn print_xcp_on_flx_buffer(
    flx_buf_id: u8,
    max_flx_len_buf: &Option<MaxFlxLenBuf>,
    lpdu_id: &Option<LpduId>,
    xcp_packet: &Option<XcpPacket>,
) {
    println!("      buffer id: {flx_buf_id}");

    if let Some(MaxFlxLenBuf {
        fixed, variable, ..
    }) = &max_flx_len_buf
    {
        if let Some(fixed) = fixed {
            println!("      buffer length: {} bytes (fixed)", fixed.length);
        }
        if let Some(variable) = variable {
            println!("      buffer length: {} bytes (variable)", variable.length);
        }
    }
    if let Some(LpduId {
        flx_slot_id,
        offset,
        cycle_repetition,
        channel,
        ..
    }) = lpdu_id
    {
        print!("      ");
        if let Some(FlxSlotId {
            fixed, variable, ..
        }) = flx_slot_id
        {
            if let Some(fixed) = fixed {
                print!("slot id: {}", fixed.slot_id);
            }
            if let Some(variable) = variable {
                if let Some(initial) = &variable.initial_value {
                    print!("slot id variable, initial value: {}", initial.slot_id);
                } else {
                    print!("slot id variable");
                }
            }
        } else {
            print!("slot id: undefined");
        }

        if let Some(CycleRepetition {
            fixed, variable, ..
        }) = cycle_repetition
        {
            if let Some(fixed) = fixed {
                print!(", cycle: {}", fixed.cycle);
            }
            if let Some(variable) = variable {
                if let Some(initial) = &variable.initial_value {
                    print!(", cycle variable, initial value: {}", initial.cycle);
                } else {
                    print!(", cycle variable");
                }
            }
        }

        if let Some(Offset {
            fixed, variable, ..
        }) = offset
        {
            if let Some(fixed) = fixed {
                print!(", offset: {}", fixed.offset);
            }
            if let Some(variable) = variable {
                if let Some(initial) = &variable.initial_value {
                    print!(", offset variable, initial value: {}", initial.offset);
                } else {
                    print!(", offset variable");
                }
            }
        }

        if let Some(Channel {
            fixed, variable, ..
        }) = channel
        {
            if let Some(fixed) = fixed {
                print!(", channel: {:?}", fixed.channel);
            }
            if let Some(variable) = variable {
                if let Some(initial) = &variable.initial_value {
                    print!(", channel variable, initial value: {:?}", initial.channel);
                } else {
                    print!(", channel variable");
                }
            }
        }
        println!();
    }

    if let Some(XcpPacket {
        cmd,
        res_err,
        ev_serv,
        daq,
        stim,
        ..
    }) = xcp_packet
    {
        println!("      packet types: ");
        if let Some(Cmd {
            packet_assignment_type,
            ..
        }) = cmd
        {
            println!("        Cmd: {packet_assignment_type:?}");
        }
        if let Some(ResErr {
            packet_assignment_type,
            ..
        }) = res_err
        {
            println!("        Res / Err: {packet_assignment_type:?}");
        }
        if let Some(EvServ {
            packet_assignment_type,
            ..
        }) = ev_serv
        {
            println!("        EvServ: {packet_assignment_type:?}");
        }
        if let Some(Daq2 {
            packet_assignment_type,
            ..
        }) = daq
        {
            println!("        Daq: {packet_assignment_type:?}");
        }
        if let Some(Stim2 {
            packet_assignment_type,
            ..
        }) = stim
        {
            println!("        Stim: {packet_assignment_type:?}");
        }
    }
}

fn print_xcp_on_tcp_ip(tcp_ip_parameters: &TCP_IP_Parameters) {
    println!("  XCP on TCP/IP");
    print_xcp_on_ip_common(
        &tcp_ip_parameters.host_name,
        &tcp_ip_parameters.address,
        &tcp_ip_parameters.ipv6,
        tcp_ip_parameters.port,
    );
}

fn print_xcp_on_udp_ip(udp_ip_parameters: &UDP_IP_Parameters) {
    println!("  XCP on UDP/IP");
    print_xcp_on_ip_common(
        &udp_ip_parameters.host_name,
        &udp_ip_parameters.address,
        &udp_ip_parameters.ipv6,
        udp_ip_parameters.port,
    );
}

fn print_xcp_on_ip_common(
    host_name: &Option<HostName>,
    address: &Option<Address2>,
    ipv6: &Option<Ipv6>,
    port: u16,
) {
    if let Some(HostName { hostname, .. }) = host_name {
        println!("    hostname: {hostname}");
    }
    if let Some(Address2 { address_v4, .. }) = address {
        println!("    address: {address_v4}");
    }
    if let Some(Ipv6 { address_v6, .. }) = ipv6 {
        println!("    address: {address_v6}");
    }
    println!("    port: {port}");
}
