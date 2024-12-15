use a2lfile::{A2lFile, Characteristic, Measurement, RecordLayout};
use std::collections::HashMap;

#[allow(dead_code)]
pub(crate) fn _search_measurements<'a>(
    a2l_file: &'a A2lFile,
    regex_strings: &[&str],
    _log_messages: &mut Vec<String>,
) -> HashMap<&'a String, &'a Measurement> {
    let mut found = HashMap::new();

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

    for module in &a2l_file.project.module {
        // search all measurements that match any of the regexes
        for measurement in &module.measurement {
            for regex in &compiled_regexes {
                if regex.is_match(&measurement.name) {
                    found.insert(&measurement.name, measurement);
                }
            }
        }
    }

    found
}

pub(crate) fn search_characteristics<'a>(
    a2l_file: &'a A2lFile,
    regex_strings: &[&str],
    _log_messages: &mut Vec<String>,
) -> HashMap<&'a String, &'a Characteristic> {
    let mut found = HashMap::new();

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

    for module in &a2l_file.project.module {
        // search all characteristics that match any of the regexes
        for characteristic in &module.characteristic {
            for regex in &compiled_regexes {
                if regex.is_match(&characteristic.name) {
                    found.insert(&characteristic.name, characteristic);
                }
            }
        }
    }

    found
}

pub(crate) fn search_reord_layout<'a>(
    a2l_file: &'a A2lFile,
    regex_strings: &[&str],
    _log_messages: &mut Vec<String>,
) -> HashMap<&'a String, &'a RecordLayout> {
    let mut found = HashMap::new();

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

    for module in &a2l_file.project.module {
        // search all characteristics that match any of the regexes
        for record_layout in &module.record_layout {
            for regex in &compiled_regexes {
                if regex.is_match(&record_layout.name) {
                    found.insert(&record_layout.name, record_layout);
                }
            }
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_measurements() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["Measurement_0."];
        let mut search_msgs = Vec::new();
        let result = _search_measurements(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 9);
    }

    #[test]
    fn test_search_characteristics() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["Characteristic_01", "Characteristic_14"];
        let mut search_msgs = Vec::new();
        let result = super::search_characteristics(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&"Characteristic_01".to_string()));
        assert!(result.contains_key(&"Characteristic_14".to_string()));
    }

    #[test]
    fn test_search_record_layout() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["RecordLayout_05"];
        let mut search_msgs = Vec::new();
        let result = super::search_reord_layout(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&"RecordLayout_05".to_string()));
    }

    #[test]
    fn test_search_measurements_no_match() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["nonexistent"];
        let mut search_msgs = Vec::new();
        let result = super::_search_measurements(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_search_characteristics_no_match() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["nonexistent"];
        let mut search_msgs = Vec::new();
        let result = super::search_characteristics(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_search_record_layout_no_match() {
        let mut load_msgs = Vec::<a2lfile::A2lError>::new();
        let a2l_file = a2lfile::load("tests/example-a2l-file.a2l", None, &mut load_msgs, false).expect("Unable to load A2L file");
        let regex_strings = vec!["nonexistent"];
        let mut search_msgs = Vec::new();
        let result = super::search_reord_layout(&a2l_file, &regex_strings, &mut search_msgs);
        assert_eq!(result.len(), 0);
    }
}
