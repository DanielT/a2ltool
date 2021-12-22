use clap::{App, Arg, ArgGroup, ArgMatches, crate_version};

use dwarf::DebugData;
use std::{time::Instant, ffi::OsStr};
use a2lfile::A2lObject;

mod ifdata;
mod dwarf;
mod update;
mod insert;
mod xcp;
mod datatype;
mod symbol;


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
        Ok(_) => {},
        Err(err) => println!("{}", err)
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
        cond_print!(verbose, now, format!("Performing consistency check for {}.", input_filename.to_string_lossy()));
        let mut log_msgs = Vec::<String>::new();
        a2l_file.check(&mut log_msgs);
        if log_msgs.len() == 0 {
            ext_println!(verbose, now, format!("Consistency check complete. No problems found."));
        } else {
            for  msg in &log_msgs {
                ext_println!(verbose, now, format!("    {}", msg));
            }
            ext_println!(verbose, now, format!("Consistency check complete. {} problems reported.", log_msgs.len()));
        }
    }

    // load elf
    let elf_info = if arg_matches.is_present("ELFFILE") {
        let elffile = arg_matches.value_of_os("ELFFILE").unwrap();
        let elf_info = DebugData::load(elffile, verbose > 0)?;
        cond_print!(verbose, now, format!("Variables and types loaded from \"{}\": {} variables available", elffile.to_string_lossy(), elf_info.variables.len()));
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
            let mut merge_a2l= a2lfile::load(mergemodule, None, &mut merge_log_msgs, strict)?;
            
            a2l_file.merge_modules(&mut merge_a2l);
            cond_print!(verbose, now, format!("Merged A2l objects from \"{}\"\n", mergemodule.to_string_lossy()));
        }
    }

    // merge at the project level
    if let Some(merge_projects) = arg_matches.values_of_os("MERGEPROJECT") {
        for mergeproject in merge_projects {
            let mut merge_log_msgs = Vec::<String>::new();
            let merge_a2l= a2lfile::load(mergeproject, None, &mut merge_log_msgs, strict)?;
    
            a2l_file.project.module.extend(merge_a2l.project.module);
            cond_print!(verbose, now, format!("Project level merge with \"{}\". There are now {} modules.\n", mergeproject.to_string_lossy(), a2l_file.project.module.len()));
        }
    }

    // merge includes
    if arg_matches.is_present("MERGEINCLUDES") {
        a2l_file.merge_includes();
        cond_print!(verbose, now, format!("Include directives have been merged\n"));
    }

    // update addresses
    if arg_matches.is_present("UPDATE") || arg_matches.is_present("SAFE_UPDATE") {
        let preserve_unknown = arg_matches.is_present("SAFE_UPDATE");
        let mut log_msgs = Vec::<String>::new();
        let summary = update::update_addresses(&mut a2l_file, &elf_info.as_ref().unwrap(), &mut log_msgs, preserve_unknown);

        for msg in log_msgs {
            cond_print!(verbose, now, msg);
        }

        cond_print!(verbose, now, format!("Address update done\nSummary:"));
        cond_print!(verbose, now, format!("   characteristic: {} updated, {} not found", summary.characteristic_updated, summary.characteristic_not_updated));
        cond_print!(verbose, now, format!("   measurement: {} updated, {} not found", summary.measurement_updated, summary.measurement_not_updated));
        cond_print!(verbose, now, format!("   axis_pts: {} updated, {} not found", summary.axis_pts_updated, summary.axis_pts_not_updated));
        cond_print!(verbose, now, format!("   blob: {} updated, {} not found", summary.blob_updated, summary.blob_not_updated));
        cond_print!(verbose, now, format!("   instance: {} updated, {} not found", summary.instance_updated, summary.instance_not_updated));
    }

    // create new items
    if arg_matches.is_present("INSERT_CHARACTERISTIC") || arg_matches.is_present("INSERT_MEASUREMENT") {
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
        
        insert::insert_items(
            &mut a2l_file,
            &elf_info.as_ref().unwrap(),
            measurement_symbols,
            characteristic_symbols
        );
    }
    if arg_matches.is_present("INSERT_CHARACTERISTIC_RANGE") || arg_matches.is_present("INSERT_MEASUREMENT_RANGE") {
        let meas_ranges = range_args_to_ranges(arg_matches.values_of("INSERT_MEASUREMENT_RANGE"));
        let char_ranges = range_args_to_ranges(arg_matches.values_of("INSERT_CHARACTERISTIC_RANGE"));

        insert::insert_ranges(
            &mut a2l_file,
            &elf_info.as_ref().unwrap(),
            meas_ranges,
            char_ranges
        );
    }
    if arg_matches.is_present("INSERT_CHARACTERISTIC_REGEX") || arg_matches.is_present("INSERT_MEASUREMENT_REGEX") {
        let meas_regexes: Vec<&str> = match arg_matches.values_of("INSERT_MEASUREMENT_REGEX") {
            Some(values) => values.collect(),
            None => Vec::new()
        };
        let char_regexes: Vec<&str> = match arg_matches.values_of("INSERT_CHARACTERISTIC_RANGE") {
            Some(values) => values.collect(),
            None => Vec::new()
        };

        insert::insert_regex(
            &mut a2l_file,
            &elf_info.as_ref().unwrap(),
            meas_regexes,
            char_regexes
        );
    }


    // remove unknown IF_DATA
    if arg_matches.is_present("IFDATA_CLEANUP") {
        a2l_file.ifdata_cleanup();
        cond_print!(verbose, now, format!("Unknown ifdata removal is done"));
    }

    // sort all elements in the file
    if arg_matches.is_present("SORT") {
        a2l_file.sort();
        cond_print!(verbose, now, format!("All objects have been sorted"));
    }

    // output
    if arg_matches.is_present("OUTPUT") {
        a2l_file.sort_new_items();
        let out_filename = arg_matches.value_of("OUTPUT").unwrap();
        let banner = &*format!("a2ltool {}", crate_version!());
        a2l_file.write(out_filename, Some(banner))?;
        cond_print!(verbose, now, format!("Output written to \"{}\"", out_filename));
    }


    cond_print!(verbose, now, format!("\nRun complete. Have a nice day!\n\n"));

    Ok(())
}

// load or create an a2l file, depending on the command line
// return the file name (a dummy value if it is created) as well as the a2l data
fn load_or_create_a2l<'a>(arg_matches: &'a ArgMatches<'a>, strict: bool, verbose: u64, now: Instant) -> Result<(&'a std::ffi::OsStr, a2lfile::A2lFile), String> {
    if let Some(input_filename) = arg_matches.value_of_os("INPUT")
    {
        let mut log_msgs = Vec::<String>::new();
        let a2lresult = a2lfile::load(input_filename, Some(ifdata::A2MLVECTOR_TEXT.to_string()), &mut log_msgs, strict);
        for msg in log_msgs {
            cond_print!(verbose, now, msg);
        }
        let a2l_file = a2lresult?;
        cond_print!(verbose, now, format!("Input \"{}\" loaded", input_filename.to_string_lossy()));
        Ok((input_filename, a2l_file))
    } else if arg_matches.is_present("CREATE") {
        // dummy file name
        let input_filename = OsStr::new("<newly created>");
        // a minimal a2l file needs only a PROJECT containing a MODULE
        let mut project = a2lfile::Project::new("new_project".to_string(), "description of project".to_string());
        project.module = vec![a2lfile::Module::new("new_module".to_string(), "".to_string())];
        let mut a2l_file = a2lfile::A2lFile::new(project);
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
    .group(
        ArgGroup::with_name("INPUT_GROUP")
            .args(&["INPUT", "CREATE"])
            .multiple(false)
            .required(true)
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
    .group(
        ArgGroup::with_name("UPDATE_GROUP")
            .args(&["UPDATE", "SAFE_UPDATE"])
            .multiple(false)
    )
    .get_matches()
}


fn range_arg_validator(arg: String) -> Result<(), String> {
    if arg.starts_with("0x") {
        match u64::from_str_radix(&arg[2..], 16) {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("\"{}\" is not a valid address: {}", arg, error))
        }
    } else {
        match arg.parse::<u64>() {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("\"{}\" is not a valid address: {}", arg, error))
        }
    }
}


fn range_args_to_ranges(args: Option<clap::Values>) -> Vec<(u64, u64)> {
    if let Some(values) = args {
        let rangevals: Vec<u64> = values.map(
            |arg| {
                if arg.starts_with("0x") {
                    u64::from_str_radix(&arg[2..], 16).unwrap()
                } else {
                    arg.parse::<u64>().unwrap()
                }
            }).collect();
        let mut addr_ranges: Vec<(u64, u64)> = Vec::new();
        for idx in (1..rangevals.len()).step_by(2) {
            addr_ranges.push( (rangevals[idx-1], rangevals[idx]) );
        }
        addr_ranges
    } else {
        Vec::new()
    }
}
