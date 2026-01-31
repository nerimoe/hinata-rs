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

    pub fn get_uid(&self) -> &[u8] {
        &self.uid
    }

    pub fn get_sak(&self) -> u8 {
        self.sak
    }

    pub fn get_aqta(&self) -> u16 {
        self.aqta
    }


    pub fn is_mifare_classic(&self) -> bool {
        (self.sak == 8 || self.sak == 0x18 || self.sak == 0x88) && self.uid.len() == 4
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