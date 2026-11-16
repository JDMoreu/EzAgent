//! `privado/resumen.md`: el final con evidencia (10.16).

use ez_core::redact::redact;

use super::super::verdict::Verdict;
use super::super::Criterion;

/// Una línea de evidencia por criterio del último veredicto limpio.
pub const MAX_EVIDENCE_LINES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionEvidence {
    pub text: String,
    pub evidence: String,
}

/// Lo que la app reúne para el resumen final; el código solo lo formatea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryData {
    pub task: String,
    pub workdir: String,
    pub approvals: i64,
    pub criteria: Vec<CriterionEvidence>,
    pub diff_stat: String,
    pub git_log: String,
}

impl SummaryData {
    /// Empareja los criterios con la evidencia del último veredicto limpio.
    #[allow(clippy::too_many_arguments)]
    pub fn from_verdict(
        task: &str,
        workdir: &str,
        approvals: i64,
        criteria: &[Criterion],
        verdict: &Verdict,
        diff_stat: &str,
        git_log: &str,
    ) -> Self {
        let criteria = criteria
            .iter()
            .enumerate()
            .map(|(index, criterion)| {
                let n = index as i64 + 1;
                let evidence = verdict
                    .criteria
                    .iter()
                    .find(|found| found.n == n)
                    .map_or_else(String::new, |found| found.evidencia.clone());
                CriterionEvidence {
                    text: criterion.text.clone(),
                    evidence,
                }
            })
            .collect();
        Self {
            task: task.to_string(),
            workdir: workdir.to_string(),
            approvals,
            criteria,
            diff_stat: diff_stat.to_string(),
            git_log: git_log.to_string(),
        }
    }

    pub fn markdown(&self) -> String {
        let mut text = format!(
            "# Resumen\n\nTarea: {}\n\nCarpeta: {}\n\nAprobaciones: {}\n\n## Criterios\n\n",
            self.task, self.workdir, self.approvals
        );
        for (index, criterion) in self.criteria.iter().enumerate() {
            text.push_str(&format!("{}. {}\n", index + 1, criterion.text));
            let lines: Vec<&str> = criterion
                .evidence
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(MAX_EVIDENCE_LINES)
                .collect();
            if lines.is_empty() {
                text.push_str("   Sin evidencia.\n");
            } else {
                for line in lines {
                    text.push_str(&format!("   - {}\n", redact(line)));
                }
            }
        }
        text.push_str("\n## Cambios\n\n");
        text.push_str(&block(&self.diff_stat));
        text.push_str("\n## Commits\n\n");
        text.push_str(&block(&self.git_log));
        text
    }
}

fn block(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "(sin datos)".to_string()
    } else {
        format!("{trimmed}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::verdict::Criterion as VerdictCriterion;

    #[test]
    fn pairs_evidence_and_redacts_it() {
        let criteria = vec![
            Criterion {
                text: "compila".to_string(),
                check: "cargo build".to_string(),
            },
            Criterion {
                text: "pasan las pruebas".to_string(),
                check: "cargo test".to_string(),
            },
        ];
        let verdict = Verdict {
            cumple: true,
            criteria: vec![
                VerdictCriterion {
                    n: 2,
                    cumple: true,
                    evidencia: "cargo test: 12 passed\napi_key=abc123".to_string(),
                },
                VerdictCriterion {
                    n: 1,
                    cumple: true,
                    evidencia: "cargo build: ok".to_string(),
                },
            ],
            findings: Vec::new(),
        };
        let data = SummaryData::from_verdict(
            "migrar",
            "C:/t",
            4,
            &criteria,
            &verdict,
            " src/lib.rs | 2 ++\n 1 file changed",
            "abc1234 migrar",
        );
        let text = data.markdown();
        assert!(text.starts_with("# Resumen\n\nTarea: migrar\n\nCarpeta: C:/t\n\nAprobaciones: 4"));
        assert!(text.contains("1. compila\n   - cargo build: ok"));
        assert!(text.contains("2. pasan las pruebas\n   - cargo test: 12 passed"));
        assert!(text.contains("- api_key=[REDACTED]"));
        assert!(text.contains("## Cambios\n\nsrc/lib.rs | 2 ++"));
        assert!(text.contains("## Commits\n\nabc1234 migrar"));
    }

    #[test]
    fn evidence_keeps_three_lines_and_notes_its_absence() {
        let data = SummaryData {
            task: "x".to_string(),
            workdir: "C:/t".to_string(),
            approvals: 0,
            criteria: vec![CriterionEvidence {
                text: "uno".to_string(),
                evidence: "a\nb\nc\nd".to_string(),
            }],
            diff_stat: String::new(),
            git_log: String::new(),
        };
        let text = data.markdown();
        assert!(text.contains("   - a\n   - b\n   - c\n"));
        assert!(!text.contains("   - d"));
        assert!(text.contains("## Cambios\n\n(sin datos)\n"));
        let empty = SummaryData {
            criteria: vec![CriterionEvidence {
                text: "dos".to_string(),
                evidence: String::new(),
            }],
            ..data
        };
        assert!(empty.markdown().contains("   Sin evidencia."));
    }
}
