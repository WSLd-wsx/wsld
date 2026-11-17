# Shared GUI assets and edition interface

`ui/` is the common manager frontend. The standalone `wsx-gui-contract` crate
defines the Rust boundary for community and Store backends. The community
backend provides the same Core inventory and terminal action as the CLI while
refusing GUI desktop launch. Desktop CLI commands are unaffected.

The `community` integration slice runs this frontend through an in-process
Tauri crate. The private Store slice implements the same interface and reuses
the existing Store entitlement provider. The currently released Tauri DLL still
exists; the new slices have not yet replaced the product entry or passed Store
package acceptance.
