//! The declared-state layer for Python (feature `python`): `ninjask.Prober`, which serves the same frames as the `ninjask-probe`
//! process (`declared::wire`): `Prober(dex).serve(frame: bytes) -> bytes`.

use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::{wire, Prober};
use crate::python::PyDex;

/// A declared-state prober over a loaded format; serves JSON request frames (`wire`) and returns the JSON response.
#[pyclass(name = "Prober", module = "ninjask", unsendable)]
pub struct PyProber {
    inner: Prober,
}

#[pymethods]
impl PyProber {
    #[new]
    fn new(dex: &PyDex) -> PyProber {
        PyProber {
            inner: Prober::new(dex.inner.clone()),
        }
    }

    /// One request frame (JSON bytes) in, one response frame out. A frame that does not parse is an error response, not an exception.
    fn serve<'py>(&mut self, py: Python<'py>, frame: &[u8]) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &wire::serve(&mut self.inner, frame))
    }
}
