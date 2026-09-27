//! The one node `vm` program the dash client-seam suites run the served page script in.
//! Included by path (`#[path = "common/vm_harness.rs"] mod vm_harness;`) by each suite that
//! drives a page function from outside the page.

/// A complete node `vm` program: the `shim` parts, one per line (the suite's DOM and `fetch`
/// stand-ins), then the served page script (read from `argv[2]`), then `driver` - which shares the page's scope, so
/// it calls the page's own functions and reads its module state directly. `filename` names the
/// program in node's stack traces.
pub fn vm_harness(shim: &[&str], driver: &str, filename: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__HARNESS_SHIM__`;
const DRIVER = String.raw`__HARNESS_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "__HARNESS_FILENAME__" });
"##;
    TEMPLATE
        .replace("__HARNESS_FILENAME__", filename)
        .replace("__HARNESS_SHIM__", &shim.join("\n"))
        .replace("__HARNESS_DRIVER__", driver)
}
