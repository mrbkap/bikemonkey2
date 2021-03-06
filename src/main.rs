use caseless::canonical_caseless_match_str;
use clap::{arg_enum, value_t, values_t, App, Arg};
use lazy_static::lazy_static;
use regex::RegexSet;
use serde::{de, Deserialize};
use serde_json::{Result, Value};
use std::{error::Error, fs::File, path::Path, time::Duration};

arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    enum Course {
        IlRegno,
        Gran,
        Medio,
        Piccollo
    }
}

arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    enum Gender {
        Male,
        Female
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Results {
    records: Vec<Value>,
    query_record_count: u32,
    total_record_count: u32,
}

#[derive(Debug, Clone)]
struct Rider<'a> {
    firstname: &'a str,
    lastname: &'a str,
    elapsedtime: Duration,
    displaytime: &'a str,
    gender: Gender,
    course: Course,
    willow_creek: bool,
    fort_ross: bool,
    bib: u64,
    _id: &'a str,
}

impl<'a> Rider<'a> {
    fn from_value(v: &'a Value) -> Result<Rider<'a>> {
        fn parse_duration(s: &str) -> Result<Duration> {
            let components: Vec<_> = s
                .split(":")
                .map(|s| s.parse::<u64>())
                .filter_map(|r| r.ok())
                .collect();
            if components.len() != 3 {
                return Err(de::Error::custom(format!("bad duration {:?}", components)));
            }

            let secs = components[0] * 60 * 60 + components[1] * 60 + components[2];
            Ok(Duration::from_secs(secs))
        }

        fn parse_course(s: &str) -> Result<(Course, bool, bool, Gender)> {
            lazy_static! {
                static ref SET: RegexSet = RegexSet::new(&[
                    "^IL REGNO",
                    "^PICCOLO",
                    "^MEDIO",
                    "^GRAN",
                    "^FAMILY",
                    "Fort Ross",
                    "WC",
                    "Male$",
                    "Female$",
                ])
                .unwrap();
            }

            let matches = SET.matches(s);
            let course = if matches.matched(0) {
                Course::IlRegno
            } else if matches.matched(1) {
                Course::Piccollo
            } else if matches.matched(2) {
                Course::Medio
            } else if matches.matched(3) {
                Course::Gran
            } else if matches.matched(4) {
                return Err(de::Error::custom("don't deal with families"));
            } else {
                return Err(de::Error::custom(format!("unknown course {:?}", s)));
            };

            let fr = matches.matched(5);
            let wc = matches.matched(6);

            let gender = if matches.matched(7) {
                Gender::Male
            } else if matches.matched(8) {
                Gender::Female
            } else {
                Gender::Male // XXX
            };

            Ok((course, wc, fr, gender))
        }

        let firstname = v["firstname"].as_str().ok_or(de::Error::custom(format!(
            "bad firstname {:?}",
            v["firstname"]
        )))?;
        let lastname = v["lastname"].as_str().ok_or(de::Error::custom(format!(
            "bad lastname {:?}",
            v["lastname"]
        )))?;

        if firstname.is_empty() && lastname.is_empty() {
            return Err(de::Error::custom("No riders with no name!"));
        }

        let displaytime = v["elapsedtime"].as_str().ok_or(de::Error::custom(format!(
            "bad time {:?}",
            v["elapsedtime"]
        )))?;
        let elapsedtime = parse_duration(displaytime)?;
        let route = v["route"]
            .as_str()
            .ok_or(de::Error::custom(format!("bad course {:?}", v["route"])))?;
        let (course, willow_creek, fort_ross, gender) = parse_course(route)?;
        let bib = v["bib"]
            .as_u64()
            .ok_or(de::Error::custom(format!("bad bibno {:?}", v["bib"])))?;
        let _id = v["_id"]
            .as_str()
            .ok_or(de::Error::custom(format!("bad id {:?}", v["_id"])))?;

        Ok(Rider {
            firstname,
            lastname,
            elapsedtime,
            displaytime,
            gender,
            course,
            willow_creek,
            fort_ross,
            bib,
            _id,
        })
    }
}

struct FilterOptions<'a> {
    courses: Option<Vec<Course>>,
    gender: Option<Gender>,
    debug: bool,
    firstname: Option<&'a str>,
    lastname: Option<&'a str>,
}

impl<'a> FilterOptions<'a> {
    fn from_arg_matches(matches: &'a clap::ArgMatches) -> FilterOptions<'a> {
        let courses = values_t!(matches.values_of("course"), Course).ok();
        let gender = value_t!(matches.value_of("gender"), Gender).ok();
        let debug = matches.is_present("debug");
        let firstname = matches.value_of("firstname");
        let lastname = matches.value_of("lastname");

        FilterOptions {
            courses,
            gender,
            debug,
            firstname,
            lastname,
        }
    }
}

struct Bikemonkey<'a> {
    riders: Vec<Rider<'a>>,
}

impl<'a> Bikemonkey<'a> {
    fn from_json(blob: &'a Results, debug: bool) -> std::io::Result<Bikemonkey<'a>> {
        let riders = blob
            .records
            .iter()
            .map(Rider::from_value)
            .filter_map(|r| {
                if debug && r.is_err() {
                    println!("Warning: bad rider found {:?}", r);
                }
                r.ok()
            })
            .collect::<Vec<_>>();

        Ok(Bikemonkey { riders })
    }

    fn filter_riders(&self, filter_options: &FilterOptions) -> Vec<&Rider> {
        let mut riders = self
            .riders
            .iter()
            .filter(|r| {
                if let Some(ref courses) = filter_options.courses {
                    if !courses.contains(&r.course) {
                        return false;
                    }
                }
                if let Some(ref gender) = filter_options.gender {
                    if r.gender != *gender {
                        return false;
                    }
                }

                true
            })
            .collect::<Vec<&Rider>>();
        riders.sort_unstable_by_key(|r| r.elapsedtime);
        riders
    }

    fn print_all(&self, filter_options: FilterOptions) {
        let riders = self.filter_riders(&filter_options);
        for (idx, r) in riders.iter().enumerate() {
            println!(
                "{} [{}, {}] {} {} ({}) {}{}{}",
                idx + 1,
                r.bib,
                r._id,
                r.firstname,
                r.lastname,
                r.displaytime,
                r.course,
                if r.willow_creek { " +WC" } else { "" },
                if r.fort_ross { " Fort Ross" } else { "" },
            )
        }
    }

    fn print_info(&self, filter_options: FilterOptions) {
        let riders = self.filter_riders(&filter_options);
        let matches = riders
            .iter()
            .enumerate()
            .filter(|&(_idx, r)| {
                if let Some(ref name) = filter_options.firstname {
                    if !canonical_caseless_match_str(&r.firstname, name) {
                        return false;
                    }
                }

                if let Some(ref name) = filter_options.lastname {
                    if !canonical_caseless_match_str(&r.lastname, name) {
                        return false;
                    }
                }

                true
            })
            .collect::<Vec<_>>();

        if matches.is_empty() {
            println!("No riders were found.");
            return;
        }

        for &(idx, rider) in matches.iter() {
            println!(
                "Rider {} {} ({}) came in position {} with a time of {} out of {} matching rider{} on \
                 the {}{}{} route",
                rider.firstname,
                rider.lastname,
                rider.bib,
                idx + 1,
                rider.displaytime,
                riders.len(),
                if riders.len() > 1 { "s" } else { "" },
                rider.course,
                if rider.willow_creek { " +WC" } else { "" },
                if rider.fort_ross { " Fort Ross" } else { "" },
            );
        }
    }
}

fn main() {
    let matches = App::new("bikemonkey2")
        .author("Blake Kaplan <mrbkap@gmail.com>")
        .arg(
            Arg::with_name("course")
                .short("c")
                .multiple(true)
                .help("Restricts which course to look at")
                .takes_value(true)
                .possible_values(&Course::variants()),
        )
        .arg(
            Arg::with_name("gender")
                .short("g")
                .multiple(false)
                .help("Restricts which genders are looked at")
                .takes_value(true)
                .possible_values(&Gender::variants()),
        )
        .arg(
            Arg::with_name("firstname")
                .short("f")
                .multiple(false)
                .help("Find a rider with a given first name")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("lastname")
                .short("l")
                .multiple(false)
                .help("Find a rider with a given last name")
                .takes_value(true)
                .required(false),
        )
        .arg(Arg::from_usage("-d, --debug   'Enable debugging'"))
        .arg(Arg::from_usage("[file]        'File to read as input'"))
        .after_help(
            "Prints info about the riders in Levi's Gran Fondo. If \
             neither -f or -l are passed, prints all riders matching \
             the other criteria. If either -f or -l are passed, prints \
             info about that rider.",
        )
        .get_matches();

    let options = FilterOptions::from_arg_matches(&matches);
    let path = Path::new(matches.value_of("file").unwrap_or("lgfresults.json"));
    let file = File::open(&path).expect(&format!("couldn't open {}", path.display()));
    let blob: Results =
        serde_json::from_reader(file).expect(&format!("error parsing {}", path.display()));
    let riders = match Bikemonkey::from_json(&blob, options.debug) {
        Err(why) => panic!("couldn't open {}: {}", path.display(), why.description()),
        Ok(riders) => riders,
    };

    if options.firstname.is_some() || options.lastname.is_some() {
        riders.print_info(options);
    } else {
        riders.print_all(options);
    }
}
