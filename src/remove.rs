use a2lfile::{A2lFile, A2lObjectName, ItemList};
use std::collections::HashSet;

/// remove items based on regex patterns
pub(crate) fn remove_items(a2l_file: &mut A2lFile, regex_strings: &[&str]) -> (Vec<String>, usize) {
    let mut log_messages: Vec<String> = Vec::new();
    let mut count = 0;

    let compiled_regexes = regex_strings
        .iter()
        .map(|re| {
            // extend the regex to match only the whole string, not just a substring
            let extended_regex = if !re.starts_with('^') && !re.ends_with('$') {
                format!("^{re}$")
            } else {
                re.to_string()
            };
            regex::Regex::new(&extended_regex).unwrap()
        })
        .collect::<Vec<_>>();

    for module in &mut a2l_file.project.module {
        let mut removed_items = HashSet::<String>::new();

        // remove all characteristics that match any of the regexes
        let mut swapped_characteristics = ItemList::with_capacity(module.characteristic.len());
        std::mem::swap(&mut module.characteristic, &mut swapped_characteristics);
        for characteristic in swapped_characteristics {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(characteristic.get_name()) {
                    log_messages.push(format!(
                        "Removed characteristic {}",
                        characteristic.get_name()
                    ));
                    removed_items.insert(characteristic.get_name().to_string());
                    removed = true;
                }
            }
            if !removed {
                module.characteristic.push(characteristic);
            }
        }

        // remove all measurements that match any of the regexes
        let mut swapped_measurements = ItemList::with_capacity(module.measurement.len());
        std::mem::swap(&mut module.measurement, &mut swapped_measurements);
        for measurement in swapped_measurements {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(measurement.get_name()) {
                    log_messages.push(format!("Removed measurement {}", measurement.get_name()));
                    removed_items.insert(measurement.get_name().to_string());
                    removed = true;
                }
            }
            if !removed {
                module.measurement.push(measurement);
            }
        }

        // remove all instances that match any of the regexes
        let mut swapped_instances = ItemList::with_capacity(module.instance.len());
        std::mem::swap(&mut module.instance, &mut swapped_instances);
        for instance in swapped_instances {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(instance.get_name()) {
                    log_messages.push(format!("Removed instance {}", instance.get_name()));
                    removed_items.insert(instance.get_name().to_string());
                    removed = true;
                }
            }
            if !removed {
                module.instance.push(instance);
            }
        }

        let mut swapped_axis_pts = ItemList::with_capacity(module.axis_pts.len());
        std::mem::swap(&mut module.axis_pts, &mut swapped_axis_pts);
        for axis_pt in swapped_axis_pts {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(axis_pt.get_name()) {
                    log_messages.push(format!("Removed axis points {}", axis_pt.get_name()));
                    removed_items.insert(axis_pt.get_name().to_string());
                    removed = true;
                }
            }
            if !removed {
                module.axis_pts.push(axis_pt);
            }
        }

        let mut swapped_blobs = ItemList::with_capacity(module.blob.len());
        std::mem::swap(&mut module.blob, &mut swapped_blobs);
        for blob in swapped_blobs {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(blob.get_name()) {
                    log_messages.push(format!("Removed blob {}", blob.get_name()));
                    removed_items.insert(blob.get_name().to_string());
                    removed = true;
                }
            }
            if !removed {
                module.blob.push(blob);
            }
        }

        clean_up_groups(module, &removed_items);
        count += removed_items.len();
    }

    (log_messages, count)
}

/// remove items based on address ranges
pub(crate) fn remove_address_ranges(
    a2l_file: &mut A2lFile,
    ranges: &[(u64, u64)],
) -> (Vec<String>, usize) {
    let mut log_messages: Vec<String> = Vec::new();
    let mut count = 0;

    for module in &mut a2l_file.project.module {
        let mut removed_items = HashSet::new();

        for range in ranges {
            let (start, end) = *range;
            // remove characteristics in the given range
            let mut swapped_characteristics = ItemList::with_capacity(module.characteristic.len());
            std::mem::swap(&mut module.characteristic, &mut swapped_characteristics);
            for characteristic in swapped_characteristics {
                let address = characteristic.address as u64;
                if address >= start && address <= end {
                    log_messages.push(format!(
                        "Removed characteristic {} at address {address:#X}",
                        characteristic.get_name()
                    ));
                    removed_items.insert(characteristic.get_name().to_string());
                } else {
                    module.characteristic.push(characteristic);
                }
            }

            // remove measurements in the given range
            let mut swapped_measurements = ItemList::with_capacity(module.measurement.len());
            std::mem::swap(&mut module.measurement, &mut swapped_measurements);
            for measurement in swapped_measurements {
                if let Some(address) = measurement
                    .ecu_address
                    .as_ref()
                    .map(|addr| addr.address as u64)
                    && address >= start
                    && address <= end
                {
                    log_messages.push(format!(
                        "Removed measurement {} at address {address:#X}",
                        measurement.get_name()
                    ));
                    removed_items.insert(measurement.get_name().to_string());
                } else {
                    module.measurement.push(measurement);
                }
            }

            // remove instances in the given range
            let mut swapped_instances = ItemList::with_capacity(module.instance.len());
            std::mem::swap(&mut module.instance, &mut swapped_instances);
            for instance in swapped_instances {
                let address = instance.start_address as u64;
                if address >= start && address <= end {
                    log_messages.push(format!(
                        "Removed instance {} at address {address:#X}",
                        instance.get_name(),
                    ));
                    removed_items.insert(instance.get_name().to_string());
                } else {
                    module.instance.push(instance);
                }
            }

            // remove axis_pts in the given range
            let mut swapped_axis_pts = ItemList::with_capacity(module.axis_pts.len());
            std::mem::swap(&mut module.axis_pts, &mut swapped_axis_pts);
            for axis_pt in swapped_axis_pts {
                let address = axis_pt.address as u64;
                if address >= start && address <= end {
                    log_messages.push(format!(
                        "Removed axis points {} at address {address:#X}",
                        axis_pt.get_name(),
                    ));
                    removed_items.insert(axis_pt.get_name().to_string());
                } else {
                    module.axis_pts.push(axis_pt);
                }
            }

            // remove blobs in the given range
            let mut swapped_blobs = ItemList::with_capacity(module.blob.len());
            std::mem::swap(&mut module.blob, &mut swapped_blobs);
            for blob in swapped_blobs {
                let address = blob.start_address as u64;
                if address >= start && address <= end {
                    log_messages.push(format!(
                        "Removed blob {} at address {address:#X}",
                        blob.get_name(),
                    ));
                    removed_items.insert(blob.get_name().to_string());
                } else {
                    module.blob.push(blob);
                }
            }
        }

        clean_up_groups(module, &removed_items);
        count += removed_items.len();
    }

    (log_messages, count)
}

fn clean_up_groups(module: &mut a2lfile::Module, removed_items: &HashSet<String>) {
    // remove references to removed items from all groups
    for group in &mut module.group {
        if let Some(ref_measurement) = &mut group.ref_measurement {
            ref_measurement
                .identifier_list
                .retain(|ident| !removed_items.contains(ident));
            if ref_measurement.identifier_list.is_empty() {
                group.ref_measurement = None;
            }
        }
        if let Some(ref_characteristic) = &mut group.ref_characteristic {
            ref_characteristic
                .identifier_list
                .retain(|ident| !removed_items.contains(ident));
            if ref_characteristic.identifier_list.is_empty() {
                group.ref_characteristic = None;
            }
        }
    }
}
