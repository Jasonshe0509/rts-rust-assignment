use std::collections::{HashMap, HashSet};
use tracing::warn;
use crate::satellite::downlink::PacketizeData;
pub struct PacketValidator{
    expected_prefixes: Vec<&'static str>,
    received: HashMap<String, HashSet<u32>>,
    reported_missing: HashMap<String, HashSet<u32>>,
    error_count: HashMap<String, u32>,
    delay_threshold_ms: i64,
}

impl PacketValidator {
    pub fn new() ->Self {
        Self{
            expected_prefixes: vec!["ANI", "RAI", "TLI"],
            received: HashMap::new(),
            reported_missing: HashMap::new(),
            error_count: HashMap::new(),
            delay_threshold_ms: 100
        }
    }
    pub fn validate_packet(&mut self,packet: &PacketizeData, drift: &i64){
        let mut trigger_rerequest = false;
        
        let (prefix, num) = split_packet_id(&packet.packet_id);

        let received_set = self.received.entry(prefix.to_string())
            .or_insert_with(HashSet::new);
        
        let prev_max = received_set.iter().max().cloned().unwrap_or(0);
        
        let reported_set = self.reported_missing.entry(prefix.to_string())
            .or_insert_with(HashSet::new);

        let count = self.error_count.entry(prefix.to_string())
            .or_insert(0);

        // Detect missing numbers between prev_max+1 and current num-1
        if num > 1 && num > prev_max + 1 {
            for missing in prev_max+1..num {
                if !reported_set.contains(&missing) {
                    warn!("⚠️ Missing packet: {}{}", prefix, missing);
                    reported_set.insert(missing);
                    *count += 1;
                    trigger_rerequest = true;
                }
            }
        } else {
            *count = 0;
            if(drift > &self.delay_threshold_ms){
                warn!("⚠️ Packet {} delayed by {} ms", packet.packet_id, drift);
                *count += 1;
                trigger_rerequest = true;
            }
        }
        received_set.insert(num);
        
        if(*count == 3){
            //simulate loss of contract
        } else if (trigger_rerequest){
            // trigger re-request
        }
    }
}

fn split_packet_id(packet_id: &str) -> (&str, u32) {
    let idx = packet_id.find(|c: char| c.is_ascii_digit()).unwrap_or(packet_id.len());
    let prefix = &packet_id[..idx];
    let num: u32 = packet_id[idx..].parse().unwrap_or(0);
    (prefix, num)
}