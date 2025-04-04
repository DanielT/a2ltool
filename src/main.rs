use clap::{Arg, ArgGroup, ArgMatches, Command, builder::ValueParser, parser::ValuesRef};

use a2lfile::{A2lError, A2lFile, A2lObject, itemlist};
use debuginfo::DebugData;
use std::{
    ffi::{OsStr, OsString},
    fmt::Display,
    time::Instant,
};
use update::{UpdateMode, UpdateType};

mod datatype;
mod debuginfo;
mod ifdata;
mod insert;
mod remove;
mod symbol;
mod update;
mod version;
mod xcp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum A2lVersion {
    V1_5_0,
    V1_5_1,
    V1_6_0,
    V1_6_1,
    V1_7_0,
    V1_7_1,
}

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
    let args = std::env::args_os();
    match core(args) {
        Ok(()) => {}
        Err(err) => {
            println!("{err}");
            std::process::exit(1);
        }
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
fn core(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let arg_matches = parse_args(args);

    let strict = *arg_matches
        .get_one::<bool>("STRICT")
        .expect("option strict must always exist");
    let check = *arg_matches
        .get_one::<bool>("CHECK")
        .expect("option check must always exist");
    let debugprint = *arg_matches
        .get_one::<bool>("DEBUGPRINT")
        .expect("option debugprint must always exist");
    let show_xcp = *arg_matches
        .get_one::<bool>("SHOW_XCP")
        .expect("option show-xcp must always exist");
    let enable_structures = *arg_matches
        .get_one::<bool>("ENABLE_STRUCTURES")
        .expect("option enable-structures must always exist");
    let cleanup = *arg_matches
        .get_one::<bool>("CLEANUP")
        .expect("option cleanup must always exist");
    let ifdata_cleanup = *arg_matches
        .get_one::<bool>("IFDATA_CLEANUP")
        .expect("option ifdata-cleanup must always exist");
    let sort = *arg_matches
        .get_one::<bool>("SORT")
        .expect("option sort must always exist");
    let merge_includes = *arg_matches
        .get_one::<bool>("MERGEINCLUDES")
        .expect("option merge-includes must always exist");
    let verbose = arg_matches.get_count("VERBOSE");
    let opt_update_type = arg_matches.get_one::<UpdateType>("UPDATE_TYPE");

    if let Some(true) = arg_matches.get_one::<bool>("SAFE_UPDATE") {
        return Err("Error: The option --update-preserve is deprecated. Use --update-mode PRESERVE instead.".to_string());
    }

    let now = Instant::now();
    cond_print!(
        verbose,
        now,
        format!("\na2ltool {}\n", env!("CARGO_PKG_VERSION"))
    );

    // load input
    let (input_filename, mut a2l_file) = load_or_create_a2l(&arg_matches, strict, verbose, now)?;
    if debugprint {
        // why not cond_print? in that case the output string must always be
        // formatted before cond_print can decide whether to print it. This can take longer than parsing the file.
        println!("================\n{a2l_file:#?}\n================\n");
    }

    // show XCP settings
    if show_xcp {
        xcp::show_settings(&a2l_file, input_filename);
    }

    // additional consistency checks
    if check {
        cond_print!(
            verbose,
            now,
            format!(
                "Performing consistency check for {}.",
                input_filename.to_string_lossy()
            )
        );
        let log_msgs = a2l_file.check();
        if log_msgs.is_empty() {
            ext_println!(
                verbose,
                now,
                "Consistency check complete. No problems found."
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

            // in strict mode, exit with error if there are any problems
            if strict {
                return Err("Exiting because strict mode is enabled.".to_string());
            }
        }
    }

    // convert/downgrade the file to some version
    if let Some(new_a2l_version) = arg_matches.get_one::<A2lVersion>("A2LVERSION") {
        version::convert(&mut a2l_file, *new_a2l_version);
    }

    let current_version = A2lVersion::from(&a2l_file);
    if enable_structures && current_version < A2lVersion::V1_7_1 {
        return Err(format!(
            "Error: The option --enable-structures requires input file version 1.7.1, but the current version is {current_version}"
        ));
    }

    // load debuginfo from an elf or pdb file
    let opt_elffile = arg_matches.get_one::<OsString>("ELFFILE");
    let opt_pdbfile = arg_matches.get_one::<OsString>("PDBFILE");
    let debuginfo = if let Some(elffile) = opt_elffile {
        Some(DebugData::load_dwarf(elffile, verbose > 0)?)
    } else if let Some(pdbfile) = opt_pdbfile {
        Some(DebugData::load_pdb(pdbfile, verbose > 0)?)
    } else {
        None
    };
    // display statistics and debug data if requested
    if let Some(debuginfo) = &debuginfo {
        // either opt_elffile or opt_pdbfile must be present if debuginfo was loaded
        let filename = opt_elffile.or(opt_pdbfile).unwrap();
        cond_print!(
            verbose,
            now,
            format!(
                "Variables and types loaded from \"{}\": {} variables available",
                filename.to_string_lossy(),
                debuginfo.variables.len()
            )
        );
        if debugprint {
            println!("================\n{debuginfo:#?}\n================\n");
        }
    }

    // merge at the module level
    if let Some(merge_modules) = arg_matches.get_many::<OsString>("MERGEMODULE") {
        for merge_module_path in merge_modules {
            let load_result = a2lfile::load(
                merge_module_path,
                Some(ifdata::A2MLVECTOR_TEXT.to_string()),
                strict,
            );

            if let Ok((mut merge_a2l, load_log_msgs)) = load_result {
                // display any log messages from the load
                for msg in load_log_msgs {
                    cond_print!(verbose, now, msg.to_string());
                }
                // merge the module
                a2l_file.merge_modules(&mut merge_a2l);
                cond_print!(
                    verbose,
                    now,
                    format!(
                        "Merged A2l objects from \"{}\"\n",
                        merge_module_path.to_string_lossy()
                    )
                );
            } else if let Ok(mut other_module) = a2lfile::load_fragment_file(
                merge_module_path,
                Some(ifdata::A2MLVECTOR_TEXT.to_string()),
            ) {
                // failed to load the file as a full A2L file, but loaded it as a module fragment
                a2l_file.project.module[0].merge(&mut other_module);
                cond_print!(
                    verbose,
                    now,
                    format!(
                        "Merged A2l objects from \"{}\"\n",
                        merge_module_path.to_string_lossy()
                    )
                );
            } else {
                return Err(format!(
                    "Failed to load \"{}\" for merging: {}\n",
                    merge_module_path.to_string_lossy(),
                    load_result.unwrap_err()
                ));
            }
        }
    }

    // merge at the project level
    if let Some(merge_projects) = arg_matches.get_many::<OsString>("MERGEPROJECT") {
        for mergeproject in merge_projects {
            let (merge_a2l, _) =
                a2lfile::load(mergeproject, None, strict).map_err(|a2lerr| a2lerr.to_string())?;

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
    if merge_includes {
        a2l_file.merge_includes();
        cond_print!(verbose, now, "Include directives have been merged\n");
    }

    // remove items if --remove was given
    if arg_matches.contains_id("REMOVE_REGEX") {
        let regexes: Vec<&str> = match arg_matches.get_many::<String>("REMOVE_REGEX") {
            Some(values) => values.map(|x| &**x).collect(),
            None => Vec::new(),
        };

        let mut log_msgs: Vec<String> = Vec::new();
        let removed_count = remove::remove_items(&mut a2l_file, &regexes, &mut log_msgs);
        for msg in log_msgs {
            cond_print!(verbose, now, msg);
        }
        cond_print!(verbose, now, format!("Removed {} items", removed_count));
    }

    if let Some(debugdata) = &debuginfo {
        // update addresses
        if let Some(update_type) = opt_update_type {
            let update_mode = arg_matches
                .get_one::<UpdateMode>("UPDATE_MODE")
                .unwrap_or(&UpdateMode::Default);

            let mut log_msgs = Vec::<String>::new();
            let (summary, strict_error) = update::update_a2l(
                &mut a2l_file,
                debugdata,
                &mut log_msgs,
                *update_type,
                *update_mode,
                enable_structures,
            );

            let display_msg = if verbose > 0 || update_mode != &UpdateMode::Strict {
                verbose
            } else {
                1
            };
            for msg in &log_msgs {
                cond_print!(display_msg, now, msg);
            }

            cond_print!(verbose, now, "Address update done\nSummary:");
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

            // in strict mode, exit with error if there are any problems
            if update_mode == &UpdateMode::Strict && strict_error {
                return Err("Exiting because strict mode is enabled.".to_string());
            }
        }

        // create new items
        if arg_matches.contains_id("INSERT_CHARACTERISTIC")
            || arg_matches.contains_id("INSERT_MEASUREMENT")
        {
            let target_group = arg_matches
                .get_one::<String>("TARGET_GROUP")
                .map(|group| &**group);

            let measurement_symbols: Vec<&str> =
                if let Some(values) = arg_matches.get_many::<String>("INSERT_MEASUREMENT") {
                    values.into_iter().map(|x| &**x).collect()
                } else {
                    Vec::new()
                };
            let characteristic_symbols: Vec<&str> =
                if let Some(values) = arg_matches.get_many::<String>("INSERT_CHARACTERISTIC") {
                    values.into_iter().map(|x| &**x).collect()
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
                enable_structures,
            );
            for msg in log_msgs {
                cond_print!(verbose, now, msg);
            }
        }

        if arg_matches.contains_id("INSERT_CHARACTERISTIC_RANGE")
            || arg_matches.contains_id("INSERT_MEASUREMENT_RANGE")
            || arg_matches.contains_id("INSERT_CHARACTERISTIC_REGEX")
            || arg_matches.contains_id("INSERT_MEASUREMENT_REGEX")
            || arg_matches.contains_id("INSERT_CHARACTERISTIC_SECTION")
            || arg_matches.contains_id("INSERT_MEASUREMENT_SECTION")
        {
            cond_print!(
                verbose,
                now,
                "Inserting new items from range, regex, or section"
            );
            let target_group = arg_matches
                .get_one::<String>("TARGET_GROUP")
                .map(|group| &**group);

            let mut meas_ranges =
                range_args_to_ranges(arg_matches.get_many::<u64>("INSERT_MEASUREMENT_RANGE"));
            let mut char_ranges =
                range_args_to_ranges(arg_matches.get_many::<u64>("INSERT_CHARACTERISTIC_RANGE"));

            let mut meas_section_ranges = section_args_to_ranges(
                arg_matches.get_many::<String>("INSERT_MEASUREMENT_SECTION"),
                debugdata,
                verbose,
            );
            let mut char_section_ranges = section_args_to_ranges(
                arg_matches.get_many::<String>("INSERT_CHARACTERISTIC_SECTION"),
                debugdata,
                verbose,
            );
            meas_ranges.append(&mut meas_section_ranges);
            char_ranges.append(&mut char_section_ranges);

            let meas_regexes: Vec<&str> =
                match arg_matches.get_many::<String>("INSERT_MEASUREMENT_REGEX") {
                    Some(values) => values.map(|x| &**x).collect(),
                    None => Vec::new(),
                };
            let char_regexes: Vec<&str> =
                match arg_matches.get_many::<String>("INSERT_CHARACTERISTIC_REGEX") {
                    Some(values) => values.map(|x| &**x).collect(),
                    None => Vec::new(),
                };

            let mut log_msgs: Vec<String> = Vec::new();
            insert::insert_many(
                &mut a2l_file,
                debugdata,
                &meas_ranges,
                &char_ranges,
                meas_regexes,
                char_regexes,
                target_group,
                &mut log_msgs,
                enable_structures,
            );
            for msg in log_msgs {
                cond_print!(verbose, now, msg);
            }
        }
    }

    // clean up unreferenced items
    if cleanup {
        a2l_file.cleanup();
        cond_print!(
            verbose,
            now,
            "Cleanup of unused items and empty groups is complete"
        );
    }

    // remove unknown IF_DATA
    if ifdata_cleanup {
        a2l_file.ifdata_cleanup();
        cond_print!(verbose, now, "Unknown ifdata removal is done");
    }

    // sort all elements in the file
    if sort {
        a2l_file.sort();
        cond_print!(verbose, now, "All objects have been sorted");
    }

    // output
    if arg_matches.contains_id("OUTPUT") {
        a2l_file.sort_new_items();
        if let Some(out_filename) = arg_matches.get_one::<OsString>("OUTPUT") {
            let banner = &*format!("a2ltool {}", env!("CARGO_PKG_VERSION"));
            a2l_file
                .write(out_filename, Some(banner))
                .map_err(|err| err.to_string())?;
            cond_print!(
                verbose,
                now,
                format!("Output written to \"{}\"", out_filename.to_string_lossy())
            );
        }
    }

    cond_print!(verbose, now, "\nRun complete. Have a nice day!\n\n");

    Ok(())
}

// load or create an a2l file, depending on the command line
// return the file name (a dummy value if it is created) as well as the a2l data
fn load_or_create_a2l(
    arg_matches: &ArgMatches,
    strict: bool,
    verbose: u8,
    now: Instant,
) -> Result<(&std::ffi::OsStr, a2lfile::A2lFile), String> {
    if let Some(input_filename) = arg_matches.get_one::<OsString>("INPUT") {
        let a2lresult = a2lfile::load(
            input_filename,
            Some(ifdata::A2MLVECTOR_TEXT.to_string()),
            strict,
        );
        let a2l_file = match a2lresult {
            Ok((a2l_file, log_msgs)) => {
                for msg in log_msgs {
                    cond_print!(verbose, now, msg.to_string());
                }
                a2l_file
            }
            Err(
                ref error @ A2lError::ParserError {
                    parser_error:
                        a2lfile::ParserError::InvalidMultiplicityNotPresent { ref block, .. },
                },
            ) if block == "A2L_FILE" => {
                // parse error in the outermost block "A2L_FILE" could indicate that this is an a2l fragment containing only the content of a MODULE
                if let Ok(module) = a2lfile::load_fragment_file(
                    input_filename,
                    Some(ifdata::A2MLVECTOR_TEXT.to_string()),
                ) {
                    // successfully loaded a module, now upgrade it to a full file
                    let mut a2l_file = a2lfile::new();
                    a2l_file.project.module[0] = module;
                    a2l_file.project.module[0].get_layout_mut().start_offset = 1;
                    a2l_file
                } else {
                    return Err(error.to_string());
                }
            }
            Err(error) => {
                return Err(error.to_string());
            }
        };

        cond_print!(
            verbose,
            now,
            format!("Input \"{}\" loaded", input_filename.to_string_lossy())
        );
        Ok((input_filename, a2l_file))
    } else if arg_matches.contains_id("CREATE") {
        // dummy file name
        let input_filename = OsStr::new("<newly created>");
        // a minimal a2l file needs only a PROJECT containing a MODULE
        let mut project = a2lfile::Project::new(
            "new_project".to_string(),
            "description of project".to_string(),
        );
        project.module = itemlist![a2lfile::Module::new(
            "new_module".to_string(),
            String::new(),
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
fn parse_args(args: impl Iterator<Item = OsString>) -> ArgMatches {
    let args = argfile::expand_args_from(args, argfile::parse_response, argfile::PREFIX)
        .unwrap_or_else(|err| {
            println!("invalid response file: {err}: {}", err.kind());
            std::env::args_os().collect()
        });
    Command::new("a2ltool")
    .version(env!("CARGO_PKG_VERSION"))
    .about("Reads, writes and modifies A2L files")
    .arg(Arg::new("INPUT")
        .help("Input A2L file")
        .index(1)
        .value_parser(ValueParser::os_string())
    )
    .arg(Arg::new("CREATE")
        .help("Create a new A2L file instead of loading an existing one")
        .long("create")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("ELFFILE")
        .help("Elf file containing symbols and address information in DWARF2+ format.\nAn exe file produced by MinGW with DWARF2 debug info can also be used.")
        .short('e')
        .long("elffile")
        .number_of_values(1)
        .value_name("ELFFILE")
        .value_parser(ValueParser::os_string())
        .alias("exefile")
        .alias("elf")
    )
    .arg(Arg::new("PDBFILE")
        .help("PDB file containig debugging information in Microsoft's Program Database format.")
        .long("pdbfile")
        .number_of_values(1)
        .value_name("PDBFILE")
        .value_parser(ValueParser::os_string())
        .alias("pdb")
    )
    .arg(Arg::new("CHECK")
        .help("Perform additional consistency checks")
        .long("check")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("CLEANUP")
        .help("Remove empty or unreferenced items")
        .short('c')
        .long("cleanup")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("MERGEMODULE")
        .help("Merge another a2l file on the MODULE level.\nThe input file and the merge file must each contain exactly one MODULE.\nThe contents will be merged so that there is one merged MODULE in the output.")
        .short('m')
        .long("merge")
        .number_of_values(1)
        .value_name("A2LFILE")
        .number_of_values(1)
        .value_parser(ValueParser::os_string())
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("MERGEPROJECT")
        .help("Merge another a2l file on the PROJECT level.\nIf the input file contains m MODULES and the merge file contains n MODULES, then there will be m + n MODULEs in the output.")
        .short('p')
        .long("merge-project")
        .number_of_values(1)
        .value_name("A2LFILE")
        .number_of_values(1)
        .value_parser(ValueParser::os_string())
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("MERGEINCLUDES")
        .help("Merge the content of all included files. The output file will contain no /include commands.")
        .short('i')
        .long("merge-includes")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("UPDATE_TYPE")
        .help("Update the A2L file based on the elf file. The update type can be one of:
  FULL: Update the address and type info of all items. This is the default.
  ADDRESSES: Update only the addresses.
The arg --elffile must be present.")
        .short('u')
        .long("update")
        .value_parser(UpdateTypeParser)
        .num_args(0..=1)
        .action(clap::ArgAction::Append)
        .default_missing_value("FULL")
        .requires("DEBUGINFO_ARGGROUP")
    )
    .arg(Arg::new("UPDATE_MODE")
        .help("Update the A2L file based on the elf file. Action can be one of:
  DEFAULT: Unknown objects are removed, invalid settings are updated.
  STRICT: Unknown objects or invalid settings trigger an error.
  PRESERVE: Unknown objects are preserved, with the address set to zero.
The arg --update must be present.")
        .long("update-mode")
        .value_parser(UpdateModeParser)
        .num_args(0..=1)
        .action(clap::ArgAction::Append)
        .default_missing_value("DEFAULT")
        .requires("DEBUGINFO_ARGGROUP")
        .requires("UPDATE_TYPE")
    )
    .arg(Arg::new("SAFE_UPDATE")
        .long("update-preserve")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
        .hide(true)
    )
    .arg(Arg::new("ENABLE_STRUCTURES")
        .help("Enable the the use of INSTANCE, TYPEDEF_STRUCTURE & co. for all operations. Requires a2l version 1.7.1")
        .short('t')
        .long("enable-structures")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
        .requires("DEBUGINFO_ARGGROUP")
    )
    .arg(Arg::new("A2LVERSION")
        .help("Convert the input file to the given version (e.g. \"1.5.1\", \"1.6.0\", etc.). This is a lossy operation, which deletes incompatible information.")
        .short('a')
        .long("a2lversion")
        .number_of_values(1)
        .value_name("A2L_VERSION")
        .value_parser(A2lVersionParser)
    )
    .arg(Arg::new("OUTPUT")
        .help("Write to the given output file. If this flag is not present, no output will be written.")
        .short('o')
        .long("output")
        .number_of_values(1)
        .value_name("A2LFILE")
        .value_parser(ValueParser::os_string())
    )
    .arg(Arg::new("STRICT")
        .help("Parse all input in strict mode. An error wil be reported if the file has any inconsistency.")
        .short('s')
        .long("strict")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("VERBOSE")
        .help("Display additional information")
        .short('v')
        .long("verbose")
        .number_of_values(0)
        .action(clap::ArgAction::Count)
    )
    .arg(Arg::new("DEBUGPRINT")
        .help("Display internal data for debugging")
        .long("debug-print")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("SORT")
        .help("Sort all the elements in the file")
        .long("sort")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("IFDATA_CLEANUP")
        .help("Remove all IF_DATA blocks that cannot be parsed according to A2ML")
        .long("ifdata-cleanup")
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("SHOW_XCP")
        .help("Display the XCP settings in the a2l file, if they exist")
        .long("show-xcp")
        .number_of_values(0)
        .action(clap::ArgAction::SetTrue)
    )
    .arg(Arg::new("INSERT_CHARACTERISTIC")
        .help("Insert a CHARACTERISTIC based on a variable in the elf file. The variable name can be complex, e.g. var.element[0].subelement")
        .short('C')
        .long("characteristic")
        .aliases(["insert-characteristic"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("VAR")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_CHARACTERISTIC_RANGE")
        .help("Insert multiple CHARACTERISTICs. All variables whose address is inside the given range will be inserted as CHARACTERISTICs.\nThis is useful in order to add all variables from a tuning data section with fixed addresses.\nExample: --characteristic-range 0x1000 0x2000")
        .long("characteristic-range")
        .aliases(["insert-characteristic-range"])
        .number_of_values(2)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("RANGE")
        .value_parser(AddressValueParser)
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_CHARACTERISTIC_REGEX")
        .help("Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as CHARACTERISTICs")
        .long("characteristic-regex")
        .aliases(["insert-characteristic-regex"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("REGEX")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_CHARACTERISTIC_SECTION")
        .help("Insert all variables from the given section as CHARACTERISTICs.")
        .long("characteristic-section")
        .aliases(["insert-characteristic-section"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("SECTION")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_MEASUREMENT")
        .help("Insert a MEASUREMENT based on a variable in the elf file. The variable name can be complex, e.g. var.element[0].subelement")
        .short('M')
        .long("measurement")
        .aliases(["insert-measurement"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("VAR")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_MEASUREMENT_RANGE")
        .help("Insert multiple MEASUREMENTs. All variables whose address is inside the given range will be inserted as MEASUREMENTs.\nExample: --measurement-range 0x1000 0x2000")
        .long("measurement-range")
        .aliases(["insert-measurement-range"])
        .number_of_values(2)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("RANGE")
        .value_parser(AddressValueParser)
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_MEASUREMENT_REGEX")
        .help("Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as MEASUREMENTs")
        .long("measurement-regex")
        .aliases(["insert-measurement-regex"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("REGEX")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("INSERT_MEASUREMENT_SECTION")
        .help("Insert all variables from the given section as MEASUREMENTs.")
        .long("measurement-section")
        .aliases(["insert-measurement-section"])
        .number_of_values(1)
        .requires("DEBUGINFO_ARGGROUP")
        .value_name("SECTION")
        .action(clap::ArgAction::Append)
    )
    .arg(Arg::new("TARGET_GROUP")
        .help("When inserting items, put them into the group named in this option. The group will be created if it doe not exist.")
        .long("target-group")
        .number_of_values(1)
        .requires("INSERT_ARGGROUP")
        .value_name("GROUP")
    )
    .arg(Arg::new("REMOVE_REGEX")
        .help("Remove any CHARACTERISTICs, MEASUREMENTs and INSTANCEs whose name matches the given regex.")
        .short('R')
        .long("remove")
        .number_of_values(1)
        .value_name("REGEX")
        .action(clap::ArgAction::Append)
    )
    .group(
        ArgGroup::new("DEBUGINFO_ARGGROUP")
            .args(["ELFFILE", "PDBFILE"])
            .multiple(false)
    )
    .group(
        ArgGroup::new("INPUT_ARGGROUP")
            .args(["INPUT", "CREATE"])
            .multiple(false)
            .required(true)
    )
    .group(
        ArgGroup::new("UPDATE_ARGGROUP")
            .args(["UPDATE_TYPE", "SAFE_UPDATE"])
            .multiple(false)
    )
    .group(
        ArgGroup::new("INSERT_ARGGROUP")
            .args(["INSERT_CHARACTERISTIC", "INSERT_CHARACTERISTIC_RANGE", "INSERT_CHARACTERISTIC_REGEX",
                "INSERT_MEASUREMENT", "INSERT_MEASUREMENT_RANGE", "INSERT_MEASUREMENT_REGEX",
                "INSERT_MEASUREMENT_SECTION", "INSERT_MEASUREMENT_SECTION", ])
            .multiple(true)
    )
    .next_line_help(false)
    .after_help("Use @filename to read a response file, which may contain any of the command line arguments.")
    .get_matches_from(args)
}

fn range_args_to_ranges(args: Option<ValuesRef<u64>>) -> Vec<(u64, u64)> {
    if let Some(values) = args {
        let rangevals: Vec<u64> = values.copied().collect();
        let mut addr_ranges: Vec<(u64, u64)> = Vec::new();
        for idx in (1..rangevals.len()).step_by(2) {
            addr_ranges.push((rangevals[idx - 1], rangevals[idx]));
        }
        addr_ranges
    } else {
        Vec::new()
    }
}

fn section_args_to_ranges(
    args: Option<ValuesRef<String>>,
    debug_data: &DebugData,
    verbose: u8,
) -> Vec<(u64, u64)> {
    if let Some(values) = args {
        let mut addr_ranges: Vec<(u64, u64)> = Vec::new();
        for section in values {
            if let Some(range) = debug_data.sections.get(section).copied() {
                addr_ranges.push(range);
            } else if verbose > 0 {
                println!("Cannot insert items from non-existent section {section}!");
            }
        }
        addr_ranges
    } else {
        Vec::new()
    }
}

#[derive(Clone)]
struct AddressValueParser;

impl clap::builder::TypedValueParser for AddressValueParser {
    type Value = u64;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        if let Some(txt) = value.to_str() {
            if let Some(hexval) = txt.strip_prefix("0x") {
                if let Ok(value) = u64::from_str_radix(hexval, 16) {
                    return Ok(value);
                }
            }
        }

        let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
        if let Some(arg) = arg {
            err.insert(
                clap::error::ContextKind::InvalidArg,
                clap::error::ContextValue::String(arg.to_string()),
            );
        }
        let strval = value.to_string_lossy();
        err.insert(
            clap::error::ContextKind::InvalidValue,
            clap::error::ContextValue::String(String::from(strval)),
        );
        Err(err)
    }
}

#[derive(Clone, Copy)]
struct A2lVersionParser;

impl clap::builder::TypedValueParser for A2lVersionParser {
    type Value = A2lVersion;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let value_str = value.to_string_lossy();
        match &*value_str {
            "1.50" | "1.5.0" => Ok(A2lVersion::V1_5_0),
            "1.51" | "1.5.1" => Ok(A2lVersion::V1_5_1),
            "1.60" | "1.6.0" => Ok(A2lVersion::V1_6_0),
            "1.61" | "1.6.1" => Ok(A2lVersion::V1_6_1),
            "1.70" | "1.7.0" => Ok(A2lVersion::V1_7_0),
            "1.71" | "1.7.1" => Ok(A2lVersion::V1_7_1),
            _ => {
                let mut err =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
                if let Some(arg) = arg {
                    err.insert(
                        clap::error::ContextKind::InvalidArg,
                        clap::error::ContextValue::String(arg.to_string()),
                    );
                }
                let strval = value.to_string_lossy();
                err.insert(
                    clap::error::ContextKind::InvalidValue,
                    clap::error::ContextValue::String(String::from(strval)),
                );
                Err(err)
            }
        }
    }
}

impl From<&A2lFile> for A2lVersion {
    fn from(a2l_file: &A2lFile) -> Self {
        if let Some(asap2_version) = &a2l_file.asap2_version {
            match (asap2_version.version_no, asap2_version.upgrade_no) {
                (1, 51) => A2lVersion::V1_5_1,
                (1, 60) => A2lVersion::V1_6_0,
                (1, 61) => A2lVersion::V1_6_1,
                (1, 70) => A2lVersion::V1_7_0,
                (1, 71) => A2lVersion::V1_7_1,
                _ => A2lVersion::V1_5_0,
            }
        } else {
            A2lVersion::V1_5_0
        }
    }
}

impl Display for A2lVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            A2lVersion::V1_5_0 => f.write_str("1.5.0"),
            A2lVersion::V1_5_1 => f.write_str("1.5.1"),
            A2lVersion::V1_6_0 => f.write_str("1.6.0"),
            A2lVersion::V1_6_1 => f.write_str("1.6.1"),
            A2lVersion::V1_7_0 => f.write_str("1.7.0"),
            A2lVersion::V1_7_1 => f.write_str("1.7.1"),
        }
    }
}

#[derive(Clone, Copy)]
struct UpdateModeParser;

impl clap::builder::TypedValueParser for UpdateModeParser {
    type Value = UpdateMode;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        match value.to_string_lossy().as_ref() {
            "DEFAULT" => Ok(UpdateMode::Default),
            "STRICT" => Ok(UpdateMode::Strict),
            "PRESERVE" => Ok(UpdateMode::Preserve),
            _ => {
                let mut err =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
                if let Some(arg) = arg {
                    err.insert(
                        clap::error::ContextKind::InvalidArg,
                        clap::error::ContextValue::String(arg.to_string()),
                    );
                }
                let strval = value.to_string_lossy();
                err.insert(
                    clap::error::ContextKind::InvalidValue,
                    clap::error::ContextValue::String(String::from(strval)),
                );
                Err(err)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct UpdateTypeParser;

impl clap::builder::TypedValueParser for UpdateTypeParser {
    type Value = UpdateType;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        match value.to_string_lossy().as_ref() {
            "FULL" => Ok(UpdateType::Full),
            "ADDRESSES" => Ok(UpdateType::Addresses),
            _ => {
                let mut err =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
                if let Some(arg) = arg {
                    err.insert(
                        clap::error::ContextKind::InvalidArg,
                        clap::error::ContextValue::String(arg.to_string()),
                    );
                }
                let strval = value.to_string_lossy();
                err.insert(
                    clap::error::ContextKind::InvalidValue,
                    clap::error::ContextValue::String(String::from(strval)),
                );
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use a2lfile::A2lObjectName;

    use super::*;

    #[test]
    fn test_option_create_output() {
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        let result = core(args.into_iter());
        // Passing the option --create should neither panic nor return an error
        // Passing the option --output should neither panic nor return an error
        // After the run, the output file should exist
        assert!(result.is_ok());
        assert!(outfile.exists());
        assert!(outfile.is_file());
    }

    #[test]
    fn test_option_input() {
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
        ];
        let result = core(args.into_iter());
        // Passing the option --input should neither panic nor return an error
        assert!(result.is_ok());
    }

    #[test]
    fn test_option_check() {
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/check_test.a2l"),
            OsString::from("--check"),
        ];
        let result = core(args.into_iter());
        // Passing the option --check should neither panic nor return an error
        // check_test.a2l has problems, but without --strict they are only warnings
        assert!(result.is_ok());

        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/check_test.a2l"),
            OsString::from("--check"),
            OsString::from("--strict"),
        ];
        let result = core(args.into_iter());
        // Passing the option --check should neither panic nor return an error
        // check_test.a2l has problems, and with --strict they are errors
        assert!(result.is_err());
    }

    #[test]
    fn test_option_elffile() {
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
        ];
        // Passing the option --elffile should neither panic nor return an error
        core(args.into_iter()).unwrap();
    }

    #[test]
    fn test_option_cleanup() {
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/cleanup_test.a2l"),
            OsString::from("--cleanup"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        // Passing the option --cleanup should neither panic nor return an error
        // cleanup_test.a2l has unused items, but --cleanup should remove them
        core(args.into_iter()).unwrap();

        let (a2l_input, _) = a2lfile::load("fixtures/a2l/cleanup_test.a2l", None, false).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert_ne!(a2l_input, a2l_output);
        // all items in cleanup_test.a2l are used
        assert!(a2l_output.project.module[0].record_layout.is_empty());
        assert!(a2l_output.project.module[0].compu_method.is_empty());
        assert!(a2l_output.project.module[0].group.is_empty());
    }

    #[test]
    fn test_option_update() {
        let tempdir = tempfile::tempdir().unwrap().into_path();

        // 1. full update
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--update"),
            OsString::from("FULL"),
            OsString::from("-v"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        // Passing the option --update should neither panic nor return an error
        // update_test.elf has symbols that can be updated in the a2l file
        core(args.into_iter()).unwrap();

        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        // the output file should have updated addresses
        let module = &a2l_output.project.module[0];
        assert_ne!(module.characteristic[0].address, 0);
        assert_ne!(
            module.measurement[0].ecu_address.as_ref().unwrap().address,
            0
        );

        // 2. address update only in strict mode on valid input
        let outfile = tempdir.join("output2.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--update"),
            OsString::from("ADDRESSES"),
            OsString::from("--update-mode"),
            OsString::from("STRICT"),
            OsString::from("-v"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert!(a2l_output.project.module[0].characteristic[0].address != 0);

        // 3. address update only in strict mode on invalid input
        let outfile = tempdir.join("output3.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test2.a2l"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test_invalid.elf"),
            OsString::from("--update"),
            OsString::from("ADDRESSES"),
            OsString::from("--update-mode"),
            OsString::from("STRICT"),
            OsString::from("-v"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        let result = core(args.into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn test_option_insert() {
        // characteristics and measurements can be inserted in several different ways:
        // - by name with --characteristic and --measurement
        // - by address range with --characteristic-range and --measurement-range
        // - by regex with --characteristic-regex and --measurement-regex
        // - by section with --characteristic-section and --measurement-section
        // The option --target-group can be used to put the inserted items into a group, and is tested here too

        // 1. insert by name
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output1.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--characteristic"),
            OsString::from("Characteristic_Value"),
            OsString::from("--measurement"),
            OsString::from("Measurement_Value"),
            OsString::from("--target-group"),
            OsString::from("TestGroup"),
            OsString::from("-v"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert_eq!(a2l_output.project.module[0].measurement.len(), 1);
        assert_eq!(a2l_output.project.module[0].characteristic.len(), 1);
        assert_eq!(a2l_output.project.module[0].group.len(), 1);
        assert_eq!(
            a2l_output.project.module[0].group[0].get_name(),
            "TestGroup"
        );
        // get the addresses of the inserted items for the second test
        let measurement_addr = a2l_output.project.module[0].measurement[0]
            .ecu_address
            .as_ref()
            .unwrap()
            .address;
        let characteristic_addr = a2l_output.project.module[0].characteristic[0].address;

        // 2. insert by address range
        let outfile = tempdir.join("output2.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--characteristic-range"),
            OsString::from(format!("0x{:x}", characteristic_addr)),
            OsString::from(format!("0x{:x}", characteristic_addr + 4)),
            OsString::from("--measurement-range"),
            OsString::from(format!("0x{:x}", measurement_addr)),
            OsString::from(format!("0x{:x}", measurement_addr + 4)),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert_eq!(a2l_output.project.module[0].measurement.len(), 1);
        assert_eq!(a2l_output.project.module[0].characteristic.len(), 1);
        assert_eq!(a2l_output.project.module[0].group.len(), 0); // no --target-group used this time

        // 3. insert by regex
        let outfile = tempdir.join("output3.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--characteristic-regex"),
            OsString::from("C.*Value"),
            OsString::from("--measurement-regex"),
            OsString::from("M.*Valu."),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert_eq!(a2l_output.project.module[0].measurement.len(), 1);
        assert_eq!(a2l_output.project.module[0].characteristic.len(), 1);

        // 4. insert by section
        let outfile = tempdir.join("output4.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--elffile"),
            OsString::from("fixtures/bin/update_test.elf"),
            OsString::from("--characteristic-section"),
            OsString::from(".data"),
            OsString::from("--measurement-section"),
            OsString::from(".bss"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        let result = core(args.into_iter());
        assert!(result.is_ok());
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert!(!a2l_output.project.module[0].measurement.is_empty());
        assert!(!a2l_output.project.module[0].characteristic.is_empty());
    }

    #[test]
    fn test_option_merge() {
        // merging can be done on the MODULE level with --merge and on the PROJECT level with --merge-project

        // 1. merge on the MODULE level
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--merge"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_input, _) = a2lfile::load("fixtures/a2l/update_test1.a2l", None, false).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        // there should be only one MODULE in the output
        assert_eq!(a2l_output.project.module.len(), 1);
        // the input file was merged with an empty file, so the output should be the same as the input
        assert_eq!(
            a2l_output.project.module[0].measurement.len(),
            a2l_input.project.module[0].measurement.len()
        );
        assert_eq!(
            a2l_output.project.module[0].characteristic.len(),
            a2l_input.project.module[0].characteristic.len()
        );
        assert_eq!(
            a2l_output.project.module[0].group.len(),
            a2l_input.project.module[0].group.len()
        );
        assert_eq!(
            a2l_output.project.module[0].record_layout.len(),
            a2l_input.project.module[0].record_layout.len()
        );
        assert_eq!(
            a2l_output.project.module[0].compu_method.len(),
            a2l_input.project.module[0].compu_method.len()
        );

        // 2. merge on the PROJECT level
        let outfile = tempdir.join("output2.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--merge-project"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_input, _) = a2lfile::load("fixtures/a2l/update_test1.a2l", None, false).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        // there should be two MODULEs in the output
        assert_eq!(a2l_output.project.module.len(), 2);
        // one of the two MODULEs in the output should be the same as the input file
        let output_module = a2l_output
            .project
            .module
            .get(a2l_input.project.module[0].get_name())
            .unwrap();
        assert_eq!(output_module, &a2l_input.project.module[0]);
    }

    #[test]
    fn test_option_remove() {
        // items can be removed by name with --remove
        let (a2l_input, _) = a2lfile::load("fixtures/a2l/update_test1.a2l", None, false).unwrap();
        // get the names of the first characteristic and measurement, so they can be removed
        let characteristic_name = a2l_input.project.module[0].characteristic[0]
            .get_name()
            .to_string();
        let measurement_name = a2l_input.project.module[0].measurement[0]
            .get_name()
            .to_string();

        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--remove"),
            OsString::from(characteristic_name),
            OsString::from("--remove"),
            OsString::from(measurement_name),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        // the output should have one less characteristic and one less measurement than the input
        assert_eq!(
            a2l_input.project.module[0].characteristic.len(),
            a2l_output.project.module[0].characteristic.len() + 1
        );
        assert_eq!(
            a2l_input.project.module[0].measurement.len(),
            a2l_output.project.module[0].measurement.len() + 1
        );
    }

    #[test]
    fn test_option_a2lversion() {
        // the a2l version can be set with --a2lversion
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("--create"),
            OsString::from("--a2lversion"),
            OsString::from("1.6.0"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();
        assert_eq!(a2l_output.asap2_version.as_ref().unwrap().version_no, 1);
        assert_eq!(a2l_output.asap2_version.as_ref().unwrap().upgrade_no, 60);

        // modify the a2l version of an existing file
        let outfile2 = tempdir.join("output2.a2l");
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--a2lversion"),
            OsString::from("1.5.0"),
            OsString::from("--output"),
            OsString::from(outfile2.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile2, None, false).unwrap();
        assert_eq!(a2l_output.asap2_version.as_ref().unwrap().version_no, 1);
        assert_eq!(a2l_output.asap2_version.as_ref().unwrap().upgrade_no, 50);
    }

    #[test]
    fn test_option_merge_includes() {
        // the content of all included files can be merged with --merge-includes
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/merge_inc_test.a2l"),
            OsString::from("--merge-includes"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let output_text = std::fs::read_to_string(outfile).unwrap();
        // the output file should not contain any /include commands
        assert!(!output_text.contains("/include"));
    }

    #[test]
    fn test_option_sort() {
        // all items in the file can be sorted with --sort
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let outfile = tempdir.join("output.a2l");
        assert!(!outfile.exists());
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/update_test1.a2l"),
            OsString::from("--sort"),
            OsString::from("--output"),
            OsString::from(outfile.clone()),
        ];
        core(args.into_iter()).unwrap();
        let (a2l_input, _) = a2lfile::load("fixtures/a2l/update_test1.a2l", None, false).unwrap();
        let (a2l_output, _) = a2lfile::load(outfile, None, false).unwrap();

        // Though sorting does not change the meaning of the file, the order of the items in the output is different.
        // That means the files are not directly equal.
        assert_ne!(a2l_input, a2l_output);
        // Comparing the number of items is a reasonable approximation to show that the content remains the same.
        assert_eq!(
            a2l_input.project.module[0].measurement.len(),
            a2l_output.project.module[0].measurement.len()
        );
        assert_eq!(
            a2l_input.project.module[0].characteristic.len(),
            a2l_output.project.module[0].characteristic.len()
        );
        assert_eq!(
            a2l_input.project.module[0].group.len(),
            a2l_output.project.module[0].group.len()
        );
        assert_eq!(
            a2l_input.project.module[0].record_layout.len(),
            a2l_output.project.module[0].record_layout.len()
        );
        assert_eq!(
            a2l_input.project.module[0].compu_method.len(),
            a2l_output.project.module[0].compu_method.len()
        );
        assert_eq!(
            a2l_input.project.module[0].instance.len(),
            a2l_output.project.module[0].instance.len()
        );
    }

    #[test]
    fn test_option_xcp() {
        // the XCP settings in the file can be displayed with --show-xcp
        let args = vec![
            OsString::from("a2ltool"),
            OsString::from("fixtures/a2l/xcp_test.a2l"),
            OsString::from("--show-xcp"),
        ];
        // Passing the option --show-xcp should neither panic nor return an error
        // The option only prints some information, so it is not possisble to check the output
        core(args.into_iter()).unwrap();
    }
}
