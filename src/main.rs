use clap::{crate_version, App, Arg, ArgGroup, ArgMatches};

use a2lfile::A2lObject;
use dwarf::DebugData;
use std::{ffi::OsStr, time::Instant};

mod datatype;
mod dwarf;
mod ifdata;
mod insert;
mod symbol;
mod update;
mod xcp;

macro_rules! cond_print {
    ($verbose:ident, $now:ident, $formatexp:expr) => {
        if $verbose == 1 {
            println!("{}", $formatexp);
        } else if $verbose >= 2 {
            for line in $formatexp.split('\n') {
                if line == "" {
                    println!("");
                } else {
                    println!("[{:9.4}ms] {}", $now.elapsed().as_secs_f64() * 1000.0, line);
                }
            }
        }
    };
}

macro_rules! ext_println {
    ($verbose:ident, $now:ident, $formatexp:expr) => {
        if $verbose <= 1 {
            println!("{}", $formatexp);
        } else {
            for line in $formatexp.split('\n') {
                if line == "" {
                    println!("");
                } else {
                    println!("[{:9.4}ms] {}", $now.elapsed().as_secs_f64() * 1000.0, line);
                }
            }
        }
    };
}

fn main() {
    match core() {
        Ok(_) => {}
        Err(err) => println!("{}", err),
    }
}

// Implement all the operations supported by a2ltool
// They will always be performed in this order:
//  1) load input
//  2) additional consistency checks
//  3) load elf
//  4) merge at the module level
//  5) merge at the project level
//  6) merge includes (flatten)
//  7) update addresses
//  8) clean up ifdata
//  9) sort the file
// 10) output
fn core() -> Result<(), String> {
    let arg_matches = get_args();

    let strict = arg_matches.is_present("STRICT");
    let verbose = arg_matches.occurrences_of("VERBOSE");
    let debugprint = arg_matches.is_present("DEBUGPRINT");

    let now = Instant::now();
    cond_print!(verbose, now, format!("\na2ltool {}\n", crate_version!()));

    // load input
    let (input_filename, mut a2l_file) = load_or_create_a2l(&arg_matches, strict, verbose, now)?;
    if debugprint {
        // why not cond_print? in that case the output string must always be
        // formatted before cond_print can decide whether to print it. This can take longer than parsing the file.
        println!("================\n{:#?}\n================\n", a2l_file)
    }

    // show XCP settings
    if arg_matches.is_present("SHOW_XCP") {
        xcp::show_settings(&a2l_file, input_filename);
    }

    // additional consistency checks
    if arg_matches.is_present("CHECK") {
        cond_print!(
            verbose,
            now,
            format!(
                "Performing consistency check for {}.",
                input_filename.to_string_lossy()
            )
        );
        let mut log_msgs = Vec::<String>::new();
        a2l_file.check(&mut log_msgs);
        if log_msgs.is_empty() {
            ext_println!(
                verbose,
                now,
                "Consistency check complete. No problems found.".to_string()
            );
        } else {
            for msg in &log_msgs {
                ext_println!(verbose, now, format!("    {}", msg));
            }
            ext_println!(
                verbose,
                now,
                format!(
                    "Consistency check complete. {} problems reported.",
                    log_msgs.len()
                )
            );
        }
    }

    // load elf
    let elf_info = if let Some(elffile) = arg_matches.value_of_os("ELFFILE") {
        let elf_info = DebugData::load(elffile, verbose > 0)?;
        cond_print!(
            verbose,
            now,
            format!(
                "Variables and types loaded from \"{}\": {} variables available",
                elffile.to_string_lossy(),
                elf_info.variables.len()
            )
        );
        if debugprint {
            println!("================\n{:#?}\n================\n", elf_info);
        }
        Some(elf_info)
    } else {
        None
    };

    // merge at the module level
    if let Some(merge_modules) = arg_matches.values_of_os("MERGEMODULE") {
        for mergemodule in merge_modules {
            let mut merge_log_msgs = Vec::<String>::new();
            let mut merge_a2l = a2lfile::load(mergemodule, None, &mut merge_log_msgs, strict)?;

            a2l_file.merge_modules(&mut merge_a2l);
            cond_print!(
                verbose,
                now,
                format!(
                    "Merged A2l objects from \"{}\"\n",
                    mergemodule.to_string_lossy()
                )
            );
        }
    }

    // merge at the project level
    if let Some(merge_projects) = arg_matches.values_of_os("MERGEPROJECT") {
        for mergeproject in merge_projects {
            let mut merge_log_msgs = Vec::<String>::new();
            let merge_a2l = a2lfile::load(mergeproject, None, &mut merge_log_msgs, strict)?;

            a2l_file.project.module.extend(merge_a2l.project.module);
            cond_print!(
                verbose,
                now,
                format!(
                    "Project level merge with \"{}\". There are now {} modules.\n",
                    mergeproject.to_string_lossy(),
                    a2l_file.project.module.len()
                )
            );
        }
    }

    // merge includes
    if arg_matches.is_present("MERGEINCLUDES") {
        a2l_file.merge_includes();
        cond_print!(
            verbose,
            now,
            "Include directives have been merged\n".to_string()
        );
    }

    if let Some(debugdata) = &elf_info {
        // update addresses
        if arg_matches.is_present("UPDATE") || arg_matches.is_present("SAFE_UPDATE") {
            let preserve_unknown = arg_matches.is_present("SAFE_UPDATE");
            let mut log_msgs = Vec::<String>::new();
            let summary =
                update::update_addresses(&mut a2l_file, debugdata, &mut log_msgs, preserve_unknown);

            for msg in log_msgs {
                cond_print!(verbose, now, msg);
            }

            cond_print!(verbose, now, "Address update done\nSummary:".to_string());
            cond_print!(
                verbose,
                now,
                format!(
                    "   characteristic: {} updated, {} not found",
                    summary.characteristic_updated, summary.characteristic_not_updated
                )
            );
            cond_print!(
                verbose,
                now,
                format!(
                    "   measurement: {} updated, {} not found",
                    summary.measurement_updated, summary.measurement_not_updated
                )
            );
            cond_print!(
                verbose,
                now,
                format!(
                    "   axis_pts: {} updated, {} not found",
                    summary.axis_pts_updated, summary.axis_pts_not_updated
                )
            );
            cond_print!(
                verbose,
                now,
                format!(
                    "   blob: {} updated, {} not found",
                    summary.blob_updated, summary.blob_not_updated
                )
            );
            cond_print!(
                verbose,
                now,
                format!(
                    "   instance: {} updated, {} not found",
                    summary.instance_updated, summary.instance_not_updated
                )
            );
        }

        // create new items
        if arg_matches.is_present("INSERT_CHARACTERISTIC")
            || arg_matches.is_present("INSERT_MEASUREMENT")
        {
            let target_group = arg_matches.value_of("TARGET_GROUP");

            let measurement_symbols: Vec<&str> =
                if let Some(values) = arg_matches.values_of("INSERT_MEASUREMENT") {
                    values.into_iter().collect()
                } else {
                    Vec::new()
                };
            let characteristic_symbols: Vec<&str> =
                if let Some(values) = arg_matches.values_of("INSERT_CHARACTERISTIC") {
                    values.into_iter().collect()
                } else {
                    Vec::new()
                };

            let mut log_msgs: Vec<String> = Vec::new();
            insert::insert_items(
                &mut a2l_file,
                debugdata,
                measurement_symbols,
                characteristic_symbols,
                target_group,
                &mut log_msgs,
            );
            for msg in log_msgs {
                cond_print!(verbose, now, msg);
            }
        }

        if arg_matches.is_present("INSERT_CHARACTERISTIC_RANGE")
            || arg_matches.is_present("INSERT_MEASUREMENT_RANGE")
            || arg_matches.is_present("INSERT_CHARACTERISTIC_REGEX")
            || arg_matches.is_present("INSERT_MEASUREMENT_REGEX")
        {
            let target_group = arg_matches.value_of("TARGET_GROUP");

            let meas_ranges =
                range_args_to_ranges(arg_matches.values_of("INSERT_MEASUREMENT_RANGE"));
            let char_ranges =
                range_args_to_ranges(arg_matches.values_of("INSERT_CHARACTERISTIC_RANGE"));
            let meas_regexes: Vec<&str> = match arg_matches.values_of("INSERT_MEASUREMENT_REGEX") {
                Some(values) => values.collect(),
                None => Vec::new(),
            };
            let char_regexes: Vec<&str> = match arg_matches.values_of("INSERT_CHARACTERISTIC_REGEX")
            {
                Some(values) => values.collect(),
                None => Vec::new(),
            };

            let mut log_msgs: Vec<String> = Vec::new();
            insert::insert_many(
                &mut a2l_file,
                debugdata,
                meas_ranges,
                char_ranges,
                meas_regexes,
                char_regexes,
                target_group,
                &mut log_msgs,
            );
            for msg in log_msgs {
                cond_print!(verbose, now, msg);
            }
        }
    }

    // remove unknown IF_DATA
    if arg_matches.is_present("CLEANUP") {
        a2l_file.cleanup();
        cond_print!(
            verbose,
            now,
            "Cleanup of unused items and empty groups is complete".to_string()
        );
    }

    // remove unknown IF_DATA
    if arg_matches.is_present("IFDATA_CLEANUP") {
        a2l_file.ifdata_cleanup();
        cond_print!(verbose, now, "Unknown ifdata removal is done".to_string());
    }

    // sort all elements in the file
    if arg_matches.is_present("SORT") {
        a2l_file.sort();
        cond_print!(verbose, now, "All objects have been sorted".to_string());
    }

    // output
    if arg_matches.is_present("OUTPUT") {
        a2l_file.sort_new_items();
        if let Some(out_filename) = arg_matches.value_of_os("OUTPUT") {
            let banner = &*format!("a2ltool {}", crate_version!());
            a2l_file.write(out_filename, Some(banner))?;
            cond_print!(
                verbose,
                now,
                format!("Output written to \"{}\"", out_filename.to_string_lossy())
            );
        }
    }

    cond_print!(
        verbose,
        now,
        "\nRun complete. Have a nice day!\n\n".to_string()
    );

    Ok(())
}

// load or create an a2l file, depending on the command line
// return the file name (a dummy value if it is created) as well as the a2l data
fn load_or_create_a2l<'a>(
    arg_matches: &'a ArgMatches<'a>,
    strict: bool,
    verbose: u64,
    now: Instant,
) -> Result<(&'a std::ffi::OsStr, a2lfile::A2lFile), String> {
    if let Some(input_filename) = arg_matches.value_of_os("INPUT") {
        let mut log_msgs = Vec::<String>::new();
        let a2lresult = a2lfile::load(
            input_filename,
            Some(ifdata::A2MLVECTOR_TEXT.to_string()),
            &mut log_msgs,
            strict,
        );
        for msg in log_msgs {
            cond_print!(verbose, now, msg);
        }
        let a2l_file = a2lresult?;
        cond_print!(
            verbose,
            now,
            format!("Input \"{}\" loaded", input_filename.to_string_lossy())
        );
        Ok((input_filename, a2l_file))
    } else if arg_matches.is_present("CREATE") {
        // dummy file name
        let input_filename = OsStr::new("<newly created>");
        // a minimal a2l file needs only a PROJECT containing a MODULE
        let mut project = a2lfile::Project::new(
            "new_project".to_string(),
            "description of project".to_string(),
        );
        project.module = vec![a2lfile::Module::new(
            "new_module".to_string(),
            "".to_string(),
        )];
        let mut a2l_file = a2lfile::A2lFile::new(project);
        // only one line break for PROJECT (after ASAP2_VERSION) instead of the default 2
        a2l_file.project.get_layout_mut().start_offset = 1;
        // only one line break for MODULE [0] instead of the default 2
        a2l_file.project.module[0].get_layout_mut().start_offset = 1;
        // also set ASAP2_VERSION 1.71
        a2l_file.asap2_version = Some(a2lfile::Asap2Version::new(1, 71));
        Ok((input_filename, a2l_file))
    } else {
        // shouldn't be able to get here, the clap config requires either INPUT or CREATE
        Err("impossible: no input filename and no --create".to_string())
    }
}

// set up the entire command line handling.
// fortunately clap makes this painless
fn get_args<'a>() -> ArgMatches<'a> {
    App::new("a2ltool")
    .version(crate_version!())
    .about("Reads, writes and modifies A2L files")
    .arg(Arg::with_name("INPUT")
        .help("Input A2L file")
        .index(1)
    )
    .arg(Arg::with_name("CREATE")
        .help("Create a new A2L file instead of loading an existing one")
        .long("create")
    )
    .arg(Arg::with_name("ELFFILE")
        .help("Elf file containing symbols and address information")
        .short("e")
        .long("elffile")
        .takes_value(true)
        .value_name("ELFFILE")
    )
    .arg(Arg::with_name("CHECK")
        .help("Perform additional consistency checks")
        .long("check")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("CLEANUP")
        .help("Remove empty or unreferenced items")
        .long("cleanup")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("MERGEMODULE")
        .help("Merge another a2l file on the MODULE level.\nThe input file and the merge file must each contain exactly one MODULE.\nThe contents will be merged so that there is one merged MODULE in the output.")
        .short("m")
        .long("merge")
        .takes_value(true)
        .value_name("A2LFILE")
        .number_of_values(1)
        .multiple(true)
    )
    .arg(Arg::with_name("MERGEPROJECT")
        .help("Merge another a2l file on the PROJECT level.\nIf the input file contains m MODULES and the merge file contains n MODULES, then there will be m + n MODULEs in the output.")
        .short("p")
        .long("merge-project")
        .takes_value(true)
        .value_name("A2LFILE")
        .number_of_values(1)
        .multiple(true)
    )
    .arg(Arg::with_name("MERGEINCLUDES")
        .help("Merge the content of all included files. The output file will contain no /include commands.")
        .short("i")
        .long("merge-includes")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("UPDATE")
        .help("Update the addresses of all objects in the A2L file based on the elf file.\nObjects that cannot be found in the elf file will be deleted.\nThe arg --elffile must be present.")
        .short("u")
        .long("update")
        .takes_value(false)
        .multiple(false)
        .requires("ELFFILE")
    )
    .arg(Arg::with_name("SAFE_UPDATE")
        .help("Update the addresses of all objects in the A2L file based on the elf file.\nObjects that cannot be found in the elf file will be preserved; their adresses will be set to zero.\nThe arg --elffile must be present.")
        .long("update-preserve")
        .takes_value(false)
        .multiple(false)
        .requires("ELFFILE")
    )
    .arg(Arg::with_name("OUTPUT")
        .help("Write to the given output file. If this flag is not present, no output will be written.")
        .short("o")
        .long("output")
        .takes_value(true)
        .value_name("A2LFILE")
        .multiple(false)
    )
    .arg(Arg::with_name("STRICT")
        .help("Parse all input in strict mode. An error wil be reported if the file has any inconsistency.")
        .short("s")
        .long("strict")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("VERBOSE")
        .help("Display additional information")
        .short("v")
        .long("verbose")
        .takes_value(false)
        .multiple(true)
    )
    .arg(Arg::with_name("DEBUGPRINT")
        .help("Display internal data for debugging")
        .long("debug-print")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("SORT")
        .help("Sort all the elements in the file")
        .long("sort")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("IFDATA_CLEANUP")
        .help("Remove all IF_DATA blocks that cannot be parsed according to A2ML")
        .long("ifdata-cleanup")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("SHOW_XCP")
        .help("Display the XCP settings in the a2l file, if they exist")
        .long("show-xcp")
        .takes_value(false)
        .multiple(false)
    )
    .arg(Arg::with_name("INSERT_CHARACTERISTIC")
        .help("Insert a CHARACTERISTIC based on a variable in the elf file. The variable name can be complex, e.g. var.element[0].subelement")
        .long("characteristic")
        .aliases(&["insert-characteristic"])
        .takes_value(true)
        .number_of_values(1)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("VAR")
    )
    .arg(Arg::with_name("INSERT_CHARACTERISTIC_RANGE")
        .help("Insert multiple CHARACTERISTICs. All variables whose address is inside the given range will be inserted as CHARACTERISTICs")
        .long("characteristic-range")
        .aliases(&["insert-characteristic-range"])
        .takes_value(true)
        .number_of_values(2)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("RANGE")
        .validator(range_arg_validator)
    )
    .arg(Arg::with_name("INSERT_CHARACTERISTIC_REGEX")
        .help("Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as CHARACTERISTICs")
        .long("characteristic-regex")
        .aliases(&["insert-characteristic-regex"])
        .takes_value(true)
        .number_of_values(1)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("REGEX")
    )
    .arg(Arg::with_name("INSERT_MEASUREMENT")
        .help("Insert a MEASUREMENT based on a variable in the elf file. The variable name can be complex, e.g. var.element[0].subelement")
        .long("measurement")
        .aliases(&["insert-measurement"])
        .takes_value(true)
        .number_of_values(1)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("VAR")
    )
    .arg(Arg::with_name("INSERT_MEASUREMENT_RANGE")
        .help("Insert multiple MEASUREMENTs. All variables whose address is inside the given range will be inserted as MEASUREMENTs")
        .long("measurement-range")
        .aliases(&["insert-measurement-range"])
        .takes_value(true)
        .number_of_values(2)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("RANGE")
        .validator(range_arg_validator)
    )
    .arg(Arg::with_name("INSERT_MEASUREMENT_REGEX")
        .help("Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as MEASUREMENTs")
        .long("measurement-regex")
        .aliases(&["insert-measurement-regex"])
        .takes_value(true)
        .number_of_values(1)
        .multiple(true)
        .requires("ELFFILE")
        .value_name("REGEX")
    )
    .arg(Arg::with_name("TARGET_GROUP")
        .help("When inserting items, put them into the group named in this option. The group will be created if it doe not exist.")
        .long("target-group")
        .takes_value(true)
        .number_of_values(1)
        .multiple(false)
        .requires("INSERT_ARGGROUP")
        .value_name("GROUP")
    )
    .group(
        ArgGroup::with_name("INPUT_ARGGROUP")
            .args(&["INPUT", "CREATE"])
            .multiple(false)
            .required(true)
     )
    .group(
        ArgGroup::with_name("UPDATE_ARGGROUP")
            .args(&["UPDATE", "SAFE_UPDATE"])
            .multiple(false)
    )
    .group(
        ArgGroup::with_name("INSERT_ARGGROUP")
            .args(&["INSERT_CHARACTERISTIC", "INSERT_CHARACTERISTIC_RANGE", "INSERT_CHARACTERISTIC_REGEX",
                "INSERT_MEASUREMENT", "INSERT_MEASUREMENT_RANGE", "INSERT_MEASUREMENT_REGEX", ])
            .multiple(true)
    )
    .get_matches()
}

fn range_arg_validator(arg: String) -> Result<(), String> {
    if let Some(hexnumber) = arg.strip_prefix("0x") {
        match u64::from_str_radix(hexnumber, 16) {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("\"{}\" is not a valid address: {}", arg, error)),
        }
    } else {
        match arg.parse::<u64>() {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("\"{}\" is not a valid address: {}", arg, error)),
        }
    }
}

fn range_args_to_ranges(args: Option<clap::Values>) -> Vec<(u64, u64)> {
    if let Some(values) = args {
        let rangevals: Vec<u64> = values
            .map(|arg| {
                if let Some(hexnumber) = arg.strip_prefix("0x") {
                    u64::from_str_radix(hexnumber, 16).unwrap()
                } else {
                    arg.parse::<u64>().unwrap()
                }
            })
            .collect();
        let mut addr_ranges: Vec<(u64, u64)> = Vec::new();
        for idx in (1..rangevals.len()).step_by(2) {
            addr_ranges.push((rangevals[idx - 1], rangevals[idx]));
        }
        addr_ranges
    } else {
        Vec::new()
    }
}
