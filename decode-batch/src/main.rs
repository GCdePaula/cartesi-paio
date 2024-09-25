use std::io::{self, Read, Write};
use std::result::Result;

use message::Batch;

fn to_json_string(str: String) -> Result<String, Box<dyn std::error::Error>> {
    let batch = Batch::from_bytes(&alloy_core::hex::decode(str.as_bytes())?)?;
    Ok(serde_json::to_string(&batch)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;

    let j = to_json_string(buffer)?;

    let mut stdout = io::stdout();
    stdout.write_all(j.as_bytes())?;
    stdout.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        // let msg = alloy_core::hex::decode("1463f9725f107358c9115bc9d86c72dd5823e9b1e60114ab7528bb862fb57e8a2bcd567a2e929a0be56a5e000a1bdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeeffab1002098bbd2ba83dc19df3577eb97ddf06421f6404afb8befead3d8b20ac025aa1e8320115b584ab3a69b698b47a518ba39fe223569126d1ae0429ecb046b91ad99ef831c").unwrap();

        // 0x1463f9725f107358c9115bc9d86c72dd5823e9b1e60114ab7528bb862fb57e8a2bcd567a2e929a0be56a5e000a1bdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeeffab1002098bbd2ba83dc19df3577eb97ddf06421f6404afb8befead3d8b20ac025aa1e8320115b584ab3a69b698b47a518ba39fe223569126d1ae0429ecb046b91ad99ef831c
        // 0x1400000000000000000000000000000000000000000114ab7528bb862fb57e8a2bcd567a2e929a0be56a5e010a0cdeadbeefdeadbeefdeadbeef20a8103e8b83a3166034ca8df57b110ffc5dfeaf326ba0081a1b69aeed2646f53d2019980a621119b0ad54dbeb6aae8c8bfad469a90c41d2a8694266e0c4fca5206c08000000000000001c

        // let batch = Batch::from_bytes(&msg).unwrap();
        let j
            = to_json_string("0x1400000000000000000000000000000000000000000114ab7528bb862fb57e8a2bcd567a2e929a0be56a5e010a0cdeadbeefdeadbeefdeadbeef20a8103e8b83a3166034ca8df57b110ffc5dfeaf326ba0081a1b69aeed2646f53d2019980a621119b0ad54dbeb6aae8c8bfad469a90c41d2a8694266e0c4fca5206c08000000000000001c".to_string()).unwrap();

        println!("{:?}", j);
    }
}
