use crate::{
    criteria::{self, Criteria, CriteriaGroup},
    errors::PrismError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader};
use tracing::warn;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureReport {
    date: String,
    extension: Vec<Extension>,
    group: Vec<Group>,
    id: Option<String>,
    measure: String,
    meta: Option<Value>,
    period: Period,
    resource_type: String,
    status: String,
    type_: String, //because "type" is a reserved keyword
}

#[derive(Debug, Deserialize, Serialize)]
struct Group {
    code: Code,
    population: Vec<Population>,
    stratifier: Vec<Stratifier>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Population {
    code: PopulationCode,
    count: u64,
    subject_results: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PopulationCode {
    coding: Vec<Coding>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Coding {
    code: String,
    system: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Period {
    end: String,
    start: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ValueQuantity {
    code: String,
    system: String,
    unit: String,
    value: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Extension {
    url: String,
    value_quantity: ValueQuantity,
}

#[derive(Debug, Deserialize, Serialize)]
struct Code {
    text: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct StratumValue {
    text: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Stratum {
    population: Vec<Population>,
    value: StratumValue,
}

#[derive(Debug, Deserialize, Serialize)]
struct Stratifier {
    code: Vec<Code>,
    stratum: Vec<Stratum>,
}

pub fn extract_criteria(measure_report: MeasureReport) -> Result<Vec<CriteriaGroup>, PrismError> {
    let mut criteria_groups: Vec<CriteriaGroup> = Vec::new();

    for g in &measure_report.group {
        let mut criteria_group = CriteriaGroup::new();
        let key = &g.code.text[..];

        for s in &g.stratifier {
            let mut criteria = Criteria::new();

            let key = &s.code.get(0).unwrap().text[..];

            for stratum in &s.stratum {
                let key = stratum.value.text.clone();
                let value = stratum.population.get(0).unwrap().count;

                criteria.insert(key, value);
            }

            criteria_group.insert(key.to_string(), criteria);
        }

        criteria_groups.push(criteria_group);
    }

    Ok(criteria_groups)
}

#[cfg(test)]
mod test {

    use super::*;
    use serde_json::json;

    const EXAMPLE_MEASURE_REPORT_BBMRI: &str =
        include_str!("../resources/test/measure_report_bbmri.json");
    const CRITERIA_GROUPS_BBMRI: &str =
        include_str!("../resources/test/criteria_groups_bbmri.json");
    const EXAMPLE_MEASURE_REPORT_DKTK: &str =
        include_str!("../resources/test/measure_report_dktk.json");
    const CRITERIA_GROUPS_DKTK: &str =
        include_str!("../resources/test/criteria_groups_dktk.json");

    #[test]
    fn test_extract_criteria_bbmri() {

        let measure_report: MeasureReport =
            serde_json::from_str(&EXAMPLE_MEASURE_REPORT_BBMRI).expect("Can't be deserialized");

        let criteria_groups =
            extract_criteria(measure_report).expect("what, no proper criteria groups");

        let criteria_groups_json = serde_json::to_string(&criteria_groups).expect("Should be JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUPS_BBMRI, criteria_groups_json);
    }

    #[test]
    fn test_extract_criteria_dktk() {
        let measure_report: MeasureReport =
            serde_json::from_str(&EXAMPLE_MEASURE_REPORT_DKTK).expect("Can't be deserialized");

        let criteria_groups =
            extract_criteria(measure_report).expect("what, no proper criteria groups");

        let criteria_groups_json = serde_json::to_string(&criteria_groups).expect("Should be JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUPS_DKTK, criteria_groups_json);
    }
}
