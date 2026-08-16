use std::collections::HashMap;
use csv::ReaderBuilder;

const OPCODE_CSV: &[u8] = include_bytes!("opcodes.csv");

#[derive(Clone)]
pub struct Disassembler {
    table: HashMap<u16, String>,
}

impl Disassembler {
    pub fn from_csv() -> Self {
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(OPCODE_CSV);

        let table = rdr.records()
            .map(|r| r.expect("CSV row read failure"))
            .map(|row| {
                let hex = u16::from_str_radix(&row[7], 16).expect("Bad hex in column 7");
                let instr = row[1].to_string(); // Instruction column
                (hex, instr)
            })
            .collect();

        Disassembler { table }
    }

    pub fn lookup(&self, opcode: u8, cb_prefix: bool, n: u8, nn: u16) -> Option<String> {
        let full_opcode : u16;
        if cb_prefix {
            full_opcode = 0xCB00 | (opcode as u16); 
        }
        else {
            full_opcode = opcode as u16;
        }
        
        let template = self.table.get(&full_opcode)?;

        // Replace common placeholders
        let mut result = template.clone();

        if result.contains("a16") {
            result = result.replace("a16", &format!("0x{:04X}", nn));
        }
        if result.contains("d16") {
            result = result.replace("d16", &format!("0x{:04X}", nn));
        }
        if result.contains("a8") {
            // Usually maps to 0xFF00 + n
            result = result.replace("a8", &format!("0x{:04X}", 0xFF00 | n as u16));
        }
        if result.contains("d8") {
            result = result.replace("d8", &format!("0x{:02X}", n));
        }
        if result.contains("r8") {
            let signed = n as i8;
            result = result.replace("r8", &format!("{:+}", signed));
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::Disassembler;

    #[test]
    fn opcode_table_is_embedded_in_the_executable() {
        let disassembler = Disassembler::from_csv();
        assert!(disassembler.lookup(0x00, false, 0, 0).is_some());
        assert!(disassembler.lookup(0x11, true, 0, 0).is_some());
    }
}
