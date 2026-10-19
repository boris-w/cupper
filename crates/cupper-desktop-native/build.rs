//! Lets the extension's Node-API symbols resolve at load time, against the
//! Node or Electron process that loads it.

fn main() {
    napi_build::setup();
}
