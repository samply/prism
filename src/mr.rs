use crate::{
    criteria::{Criteria, Stratifiers},
    errors::PrismError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
struct ValueRatio {
    denominator: Value,
    numerator: Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Extension {
    url: String,
    value_quantity: Option<ValueQuantity>,
    value_ratio: Option<ValueRatio>,
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
    stratum: Option<Vec<Stratum>>,
}

pub fn extract_criteria(measure_report: MeasureReport) -> Result<Stratifiers, PrismError> {
    //let mut criteria_groups: CriteriaGroups = CriteriaGroups::new();

    let mut stratifiers = Stratifiers::new();

    for g in &measure_report.group {

        for s in &g.stratifier {
            let mut criteria = Criteria::new();

            let criteria_key = s
                .code
                .first()
                .ok_or_else(|| PrismError::ParsingError("Missing criterion key".into()))?
                .text
                .clone();
            if let Some(strata) = &s.stratum {
                for stratum in strata {
                    let stratum_key = stratum.value.text.clone();
                    let value = stratum
                        .population
                        .first()
                        .ok_or_else(|| PrismError::ParsingError("Missing criterion count".into()))?
                        .count;

                    criteria.insert(stratum_key, value);
                }
            }
            stratifiers.insert(criteria_key, criteria);
        }
    }
    Ok(stratifiers)
}

#[cfg(test)]
mod test {

    use super::*;

    const EXAMPLE_MEASURE_REPORT_BBMRI: &str =
        include_str!("../resources/test/measure_report_bbmri.json");
    const CRITERIA_GROUPS_BBMRI: &str =
        include_str!("../resources/test/criteria_groups_bbmri.json");
    const EXAMPLE_MEASURE_REPORT_DKTK: &str =
        include_str!("../resources/test/measure_report_dktk.json");
    const CRITERIA_GROUPS_DKTK: &str = include_str!("../resources/test/criteria_groups_dktk.json");

    #[test]
    fn test_extract_criteria_bbmri() {
        let measure_report: MeasureReport =
            serde_json::from_str(&EXAMPLE_MEASURE_REPORT_BBMRI).expect("Can't be deserialized");

        let stratifiers =
            extract_criteria(measure_report).expect("what, no proper criteria groups");

        let stratifiers_json = serde_json::to_string(&stratifiers).expect("Should be JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUPS_BBMRI, stratifiers_json);
    }

    #[test]
    fn test_extract_criteria_dktk() {
        let measure_report: MeasureReport =
            serde_json::from_str(&EXAMPLE_MEASURE_REPORT_DKTK).expect("Can't be deserialized");

        let stratifiers =
            extract_criteria(measure_report).expect("what, no proper criteria groups");

        let stratifiers_json = serde_json::to_string(&stratifiers).expect("Should be JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUPS_DKTK, stratifiers_json);
    }
}
