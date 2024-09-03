use a2lfile::A2lFile;

pub(crate) fn remove_items(
    a2l_file: &mut A2lFile,
    regex_strings: &[&str],
    log_messages: &mut Vec<String>,
) -> usize {
    let mut removed_items = std::collections::HashSet::<String>::new();

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
        // remove all characteristics that match any of the regexes
        let mut swapped_characteristics = Vec::with_capacity(module.characteristic.len());
        std::mem::swap(&mut module.characteristic, &mut swapped_characteristics);
        for characteristic in swapped_characteristics {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(&characteristic.name) {
                    removed_items.insert(characteristic.name.clone());
                    removed = true;
                    log_messages.push(format!("Removed characteristic {}", characteristic.name));
                }
            }
            if !removed {
                module.characteristic.push(characteristic);
            }
        }

        // remove all measurements that match any of the regexes
        let mut swapped_measurements = Vec::with_capacity(module.measurement.len());
        std::mem::swap(&mut module.measurement, &mut swapped_measurements);
        for measurement in swapped_measurements {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(&measurement.name) {
                    removed_items.insert(measurement.name.clone());
                    removed = true;
                    log_messages.push(format!("Removed measurement {}", measurement.name));
                }
            }
            if !removed {
                module.measurement.push(measurement);
            }
        }

        // remove all instances that match any of the regexes
        let mut swapped_instances = Vec::with_capacity(module.instance.len());
        std::mem::swap(&mut module.instance, &mut swapped_instances);
        for instance in swapped_instances {
            let mut removed = false;
            for regex in &compiled_regexes {
                if regex.is_match(&instance.name) {
                    removed_items.insert(instance.name.clone());
                    removed = true;
                    log_messages.push(format!("Removed instance {}", instance.name));
                }
            }
            if !removed {
                module.instance.push(instance);
            }
        }

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

    removed_items.len()
}
