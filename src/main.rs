#[macro_use]
extern crate clap;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use clap::{App, Arg};
use std::time::Duration;
use serde_json::*;
use serde::de;
use std::fs::File;
use std::path::Path;
use std::error::Error;

arg_enum! {
    #[derive(Debug, PartialEq)]
    enum Course {
        IlRegno,
        Gran,
        Medio,
        Piccollo,
        Family
    }
}

arg_enum! {
    #[derive(Debug, PartialEq)]
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

#[derive(Debug)]
struct Rider {
    firstname: String,
    lastname: String,
    elapsedtime: Duration,
    displaytime: String,
    gender: Gender,
    course: Course,
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

        fn parse_course(s: Vec<&str>) -> Result<(Course, Gender)> {
            let mut idx = 0;
            let course = match s[0] {
                "IL" => {
                    if s.len() < 3 {
                        return Err(de::Error::custom(format!("bad route {:?}", s)));
                    }
                    idx += 1;
                    Course::IlRegno
                }
                "PICCOLO" => Course::Piccollo,
                "MEDIO" => Course::Medio,
                "GRAN" => {
                    if s[1] == "Fort" {
                        idx += 2;
                    }
                    Course::Gran
                }
                "FAMILY" => return Err(de::Error::custom("don't deal with families")), // meh
                _ => return Err(de::Error::custom(format!("unknown course {:?}", s))),
            };

            idx += 1;
            if idx < s.len() {
                if s[idx] == "WC" || s[idx] == "TANDEM" {
                    idx += 1;
                }
            }

            let gender = match if idx < s.len() { Some(&s[idx]) } else { None } {
                Some(&"Male") | None => Gender::Male,
                Some(&"Female") => Gender::Female,
                Some(other) => return Err(de::Error::custom(format!("bad gender {:?}", other))),
            };

            Ok((course, gender))
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
        let (course, gender) = parse_course(route)?;
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
            bib: bib,
            _id: String::from(id),
        })
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
        .arg(Arg::from_usage("-d, --debug   'Enable debugging'"))
        .arg(Arg::from_usage("[file]        'File to read as input'"))
        .get_matches();

    let courses = values_t!(matches.values_of("course"), Course).ok();
    let gender = value_t!(matches.value_of("gender"), Gender).ok();
    let debug = matches.is_present("debug");

    let path = Path::new(matches.value_of("file").unwrap_or("lgfresults.json"));
    let file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", path.display(), why.description()),
        Ok(file) => file,
    };
    let blob: Results = serde_json::from_reader(file).unwrap();
    let mut maybe_riders = blob.records
        .iter()
        .map(Rider::from_value)
        .filter_map(|r| {
            if debug && r.is_err() {
                println!("Warning: bad rider found {:?}", r);
            }
            r.ok()
        })
        .collect::<Vec<_>>();

    let mut riders = maybe_riders
        .iter_mut()
        .filter(|r| {
            if let Some(ref courses) = courses {
                if !courses.contains(&r.course) {
                    return false;
                }
            }
            if let Some(ref gender) = gender {
                if r.gender != *gender {
                    return false;
                }
            }

            true
        })
        .collect::<Vec<_>>();
    riders.sort_unstable_by_key(|r| r.elapsedtime);

    for (idx, r) in riders.iter().enumerate() {
        println!(
            "{} [{}, {}] {} {} ({:?})",
            idx + 1,
            r.bib,
            r._id,
            r.firstname,
            r.lastname,
            r.displaytime
        )
    }
}
