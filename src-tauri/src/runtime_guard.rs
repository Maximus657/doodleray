const MISSING_RUNTIME_MESSAGE: &str =
    "DoodleRay cannot start because Windows runtime resources are missing. Restore the runtime sidecars before running the app.";

pub fn message(runtime_resources_suppressed: bool) -> Option<&'static str> {
    runtime_resources_suppressed.then_some(MISSING_RUNTIME_MESSAGE)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_execution_when_runtime_resources_were_suppressed() {
        assert_eq!(
            super::message(true),
            Some("DoodleRay cannot start because Windows runtime resources are missing. Restore the runtime sidecars before running the app.")
        );
        assert_eq!(super::message(false), None);
    }
}
