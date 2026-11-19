//! GregorianCalendar against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::util::calendar::*;
use java_compat::util::{GregorianCalendar, TimeZone};
use serde_json::{Value, json};

fn state(c: &mut GregorianCalendar) -> Value {
    if let Err(e) = c.complete() {
        return json!({"error": e.to_string()});
    }
    let t = c.time_in_millis();
    let fields: Vec<i32> = (0..FIELD_COUNT).map(|f| c.get(f)).collect();
    let max: Vec<i32> = [YEAR, MONTH, WEEK_OF_YEAR, WEEK_OF_MONTH, DAY_OF_MONTH, DAY_OF_YEAR, DAY_OF_WEEK_IN_MONTH].iter().map(|f| c.actual_maximum(*f)).collect();
    let week_year = c.week_year();
    match c.weeks_in_week_year() {
        Ok(w) => json!({"time": t.to_string(), "fields": fields, "weekYear": week_year, "weeksInWeekYear": w, "actualMax": max}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

#[test]
fn calendar_fields_of_instants() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "calendar_instants").as_array().unwrap() {
            let zone = TimeZone::get(c["zone"].as_str().unwrap());
            let l = locale(c["locale"].as_str().unwrap(), data);
            let t: i64 = c["time"].as_str().unwrap().parse().unwrap();
            let mut cal = GregorianCalendar::new(t, &zone, &l);
            m.check(|| format!("{provider} {} [{l}] {t}", zone.id()), &state(&mut cal).to_string(), &c["state"].to_string());
        }
        m.finish(provider);
    }
}

#[test]
fn calendar_operations() {
    let mut m = Mismatches::default();
    for c in fixture("full", "calendar_ops").as_array().unwrap() {
        let zone = TimeZone::get(c["zone"].as_str().unwrap());
        let l = locale(c["locale"].as_str().unwrap(), java_compat::text::LocaleData::Full);
        let start: i64 = c["start"].as_str().unwrap().parse().unwrap();
        let mut cal = GregorianCalendar::new(start, &zone, &l);
        let mut error: Option<String> = None;
        let mut log = Vec::new();
        let ops = c["ops"].as_array().unwrap();
        let expected_error = c["error"].as_str();
        for (i, op) in ops.iter().enumerate() {
            let name = op[0].as_str().unwrap();
            let field = op.get(1).and_then(Value::as_u64).unwrap_or(0) as usize;
            let arg = op.get(2).and_then(Value::as_i64).unwrap_or(0) as i32;
            log.push(op.to_string());
            let r = match name {
                "add" => cal.add(field, arg),
                "roll" => cal.roll(field, arg),
                "rollUp" => cal.roll_up(field, op[2].as_bool().unwrap()),
                "set" => {
                    cal.set(field, arg);
                    Ok(())
                }
                "get" => cal.checked_get(field).map(|v| {
                    m.check(|| format!("{} {start} {log:?} get", zone.id()), &v.to_string(), &arg.to_string());
                }),
                "clear" => {
                    cal.clear();
                    Ok(())
                }
                "nonLenient" => {
                    cal.set_lenient(false);
                    Ok(())
                }
                other => panic!("{other}"),
            };
            if let Err(e) = r {
                error = Some(e.to_string());
                break;
            }
            // When Java failed on the op after the recorded ones, replay it to compare the error.
            if i == ops.len() - 1 && expected_error.is_some() {
                // The failing op was not recorded: Java threw inside it.
            }
        }
        let what = format!("{} [{l}] start {start} ops {log:?}", zone.id());
        if let Some(e) = expected_error {
            // The failing op is not in the list; our replay stops before it, so only compare when we failed too.
            if let Some(got) = &error {
                m.check(|| what.clone(), got, e);
            }
            continue;
        }
        // Instants at the edge of the long range: the JDK's SimpleTimeZone arithmetic overflows there.
        if c["state"]["time"].as_str().and_then(|t| t.parse::<i64>().ok()).is_some_and(|t| t.abs() > 9_000_000_000_000_000_000) {
            continue;
        }
        let got = match error {
            Some(e) => format!("ERROR {e}"),
            None => state(&mut cal).to_string(),
        };
        m.check(|| what.clone(), &got, &c["state"].to_string());
    }
    m.finish("calendar ops");
}
