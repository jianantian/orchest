use pyo3::prelude::*;

#[pymodule]
fn agent_runtime_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = m;
    Ok(())
}
