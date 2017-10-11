extern crate caseless;
#[macro_use]
extern crate clap;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use caseless::canonical_caseless_match_str;
use clap::{App, Arg};
use std::time::Duration;
use serde_json::*;
use serde::de;
use std::fs::File;
use std::path::Path;
use std::error::Error;

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
struct Rider {
    firstname: String,
    lastname: String,
    elapsedtime: Duration,
    displaytime: String,
    gender: Gender,
    course: Course,
    willow_creek: bool,
    fort_ross: bool,
    bib: u64,
    _id: String,
}

impl Rider {
    fn from_value(v: &Value) -> Result<Rider> {
        fn parse_duration(s: &str) -> Result<Duration> {
            let components: Vec<_> = s.split(":")
                .map(|s| s.parse::<u64>())
                .filter_map(|r| r.ok())
                .collect();
            if components.len() != 3 {
                return Err(de::Error::custom(format!("bad duration {:?}", components)));
            }

            let secs = components[0] * 60 * 60 + components[1] * 60 + components[2];
            Ok(Duration::from_secs(secs))
        }

        fn parse_course(s: Vec<&str>) -> Result<(Course, bool, bool, Gender)> {
            let mut idx = 0;
            let (course, fr) = match s[0] {
                "IL" => {
                    if s.len() < 3 {
                        return Err(de::Error::custom(format!("bad route {:?}", s)));
                    }
                    idx += 1;
                    (Course::IlRegno, false)
                }
                "PICCOLO" => (Course::Piccollo, false),
                "MEDIO" => (Course::Medio, false),
                "GRAN" => {
                    let mut fr = false;
                    if s[1] == "Fort" {
                        idx += 2;
                        fr = true;
                    }
                    (Course::Gran, fr)
                }
                "FAMILY" => return Err(de::Error::custom("don't deal with families")), // meh
                _ => return Err(de::Error::custom(format!("unknown course {:?}", s))),
            };

            let mut wc = false;
            idx += 1;
            if idx < s.len() {
                if s[idx] == "WC" {
                    wc = true;
                    idx += 1;
                } else if s[idx] == "TANDEM" {
                    idx += 1;
                }
            }

            let gender = match if idx < s.len() { Some(&s[idx]) } else { None } {
                Some(&"Male") | None => Gender::Male,
                Some(&"Female") => Gender::Female,
                Some(other) => return Err(de::Error::custom(format!("bad gender {:?}", other))),
            };

            Ok((course, wc, fr, gender))
        }

        let firstname = v["firstname"].as_str().ok_or(de::Error::custom(
            format!("bad firstname {:?}", v["firstname"]),
        ))?;
        let lastname = v["lastname"].as_str().ok_or(de::Error::custom(
            format!("bad lastname {:?}", v["lastname"]),
        ))?;

        if firstname.is_empty() && lastname.is_empty() {
            return Err(de::Error::custom("No riders with no name!"));
        }

        let t = v["elapsedtime"].as_str().ok_or(de::Error::custom(
            format!("bad time {:?}", v["elapsedtime"]),
        ))?;
        let time = parse_duration(t)?;
        let route = v["route"]
            .as_str()
            .ok_or(de::Error::custom(format!("bad course {:?}", v["route"])))?
            .split(" ")
            .collect::<Vec<_>>();
        let (course, wc, fr, gender) = parse_course(route)?;
        let bib = v["bib"]
            .as_u64()
            .ok_or(de::Error::custom(format!("bad bibno {:?}", v["bib"])))?;
        let id = v["_id"]
            .as_str()
            .ok_or(de::Error::custom(format!("bad id {:?}", v["_id"])))?;

        Ok(Rider {
            firstname: String::from(firstname),
            lastname: String::from(lastname),
            elapsedtime: time,
            displaytime: String::from(t),
            gender: gender,
            course: course,
            willow_creek: wc,
            fort_ross: fr,
            bib: bib,
            _id: String::from(id),
        })
    }
}

struct FilterOptions {
    courses: Option<Vec<Course>>,
    gender: Option<Gender>,
    debug: bool,
    firstname: Option<String>,
    lastname: Option<String>,
}

impl FilterOptions {
    fn from_arg_matches(matches: &clap::ArgMatches) -> FilterOptions {
        let courses = values_t!(matches.values_of("course"), Course).ok();
        let gender = value_t!(matches.value_of("gender"), Gender).ok();
        let debug = matches.is_present("debug");
        let firstname = match matches.value_of("firstname") {
            Some(name) => Some(String::from(name)),
            None => None,
        };
        let lastname = match matches.value_of("lastname") {
            Some(name) => Some(String::from(name)),
            None => None,
        };

        FilterOptions {
            courses: courses,
            gender: gender,
            debug: debug,
            firstname: firstname,
            lastname: lastname,
        }
    }
}

struct Bikemonkey {
    riders: Vec<Rider>,
}

impl Bikemonkey {
    fn from_json(path: &Path, debug: bool) -> std::io::Result<Bikemonkey> {
        let file = File::open(&path)?;
        let blob: Results = serde_json::from_reader(file)?;
        let maybe_riders = blob.records
            .iter()
            .map(Rider::from_value)
            .filter_map(|r| {
                if debug && r.is_err() {
                    println!("Warning: bad rider found {:?}", r);
                }
                r.ok()
            })
            .collect::<Vec<_>>();

        Ok(Bikemonkey {
            riders: maybe_riders,
        })
    }

    fn filter_riders(&self, filter_options: &FilterOptions) -> Vec<&Rider> {
        let mut riders = self.riders
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
                match filter_options.firstname {
                    Some(ref name) => if !canonical_caseless_match_str(&r.firstname, name) {
                        return false;
                    },
                    _ => {}
                }

                match filter_options.lastname {
                    Some(ref name) => if !canonical_caseless_match_str(&r.lastname, name) {
                        return false;
                    },
                    _ => {}
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
                "Rider {} {} came in position {} with a time of {} out of {} matching rider{} on \
                 the {}{}{} route",
                rider.firstname,
                rider.lastname,
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
    let riders = match Bikemonkey::from_json(&path, options.debug) {
        Err(why) => panic!("couldn't open {}: {}", path.display(), why.description()),
        Ok(riders) => riders,
    };

    if options.firstname.is_some() || options.lastname.is_some() {
        riders.print_info(options);
    } else {
        riders.print_all(options);
    }
}
