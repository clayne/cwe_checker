/*!
The main library of the cwe_checker containing all CWE checks and analysis modules.

# What is the cwe_checker

The cwe_checker is a tool for finding common bug classes on binaries using static analysis.
These bug classes are formally known as [Common Weakness Enumerations](https://cwe.mitre.org/) (CWEs).
Its main goal is to aid analysts to quickly find potentially vulnerable code paths.

Currently its main focus are ELF binaries that are commonly found on Linux and Unix operating systems.
The cwe_checker uses [Ghidra](https://ghidra-sre.org/) to disassemble binaries into one common intermediate representation
and implements its own analyses on this IR.
Hence, the analyses can be run on most CPU architectures that Ghidra can disassemble,
which makes the cwe_checker a valuable tool for firmware analysis.

# Usage

If the cwe_checker is installed locally, just run
```sh
cwe_checker BINARY
```
If you want to use the official docker image, you have to mount the input binary into the docker container, e.g.
```sh
docker run --rm -v $(pwd)/BINARY:/input ghcr.io/fkie-cad/cwe_checker /input
```
One can modify the behaviour of the cwe_checker through the command line.
Use the `--help` command line option for more information.
One can also provide a custom configuration file to modify the behaviour of each check
through the `--config` command line option.
Start by taking a look at the standard configuration file located at `src/config.json`
and read the [check-specific documentation](crate::checkers) for more details about each field in the configuration file.

There is _experimental_ support for the analysis of Linux loadable kernel modules
(LKMs). *cwe_checker* will recognize if you pass an LKM and will execute a
subset of the CWE checks available for user-space programs. Analyses are
configurable via a separate configuration file at `src/lkm_config.json`.

## For bare-metal binaries

The cwe_checker offers experimental support for analyzing bare-metal binaries.
For that, one needs to provide a bare metal configuration file via the `--bare-metal-config` command line option.
An example for such a configuration file can be found at `bare_metal/stm32f407vg.json`
(which was created and tested for an STM32F407VG MCU).

For more information on the necessary fields of the configuration file
and the assumed memory model when analyzing bare metal binaries
see the [configuration struct documentation](crate::utils::binary::BareMetalConfig).

# Integration into other tools

### Integration into Ghidra

To import the results of the cwe_checker as bookmarks and end-of-line comments into Ghidra,
one can use the Ghidra script located at `ghidra_plugin/cwe_checker_ghidra_plugin.py`.
Detailed usage instructions are contained in the file.

### Integration into FACT

[FACT](https://github.com/fkie-cad/FACT_core) already contains a ready-to-use cwe_checker plugin,
which lets you run the cwe_checker and view its result through the FACT user interface.

# Further documentation

You can find out more information about each check, including known false positives and false negatives,
by reading the check-specific module documentation in the [`checkers`] module.
*/

pub mod abstract_domain;
pub mod analysis;
pub mod checkers;
pub mod ghidra_pcode;
pub mod intermediate_representation;
pub mod pipeline;
pub mod utils;

mod prelude {
    pub use apint::Width;
    pub use serde::{Deserialize, Serialize};

    pub use crate::intermediate_representation::{Bitvector, BitvectorExtended, ByteSize};
    pub use crate::intermediate_representation::{Term, Tid};
    pub use crate::pipeline::AnalysisResults;

    pub use anyhow::Context as _;
    pub use anyhow::{anyhow, Error};
}
