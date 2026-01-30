#[derive(Debug, PartialEq)]
pub enum PassiveTarget {
    Iso14443a(Iso14443a),
    Felica(Felica)
}

#[derive(Debug, PartialEq)]
pub struct Iso14443a {
    uid: Vec<u8>,
    sak: u8,
    aqta: u16
}

impl Iso14443a {
    pub fn new(uid: Vec<u8>, sak: u8, aqta: u16) -> Self {
        Self {
            uid,
            sak,
            aqta
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Felica {
    idm: [u8; 8],
    pmm: [u8; 8],
    system_codes: Vec<u16>
}

impl Felica {
    pub fn new(idm: [u8; 8], pmm: [u8; 8], system_codes: Vec<u16>) -> Self {
        Self {
            idm,
            pmm,
            system_codes
        }
    }
    
    pub fn get_idm(&self) -> &[u8; 8] {
        &self.idm
    }
    
    pub fn get_pmm(&self) -> &[u8; 8] {
        &self.pmm
    }
    
    pub fn get_system_codes(&self) -> &[u16] {
        &self.system_codes
    }
}