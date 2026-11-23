//! The single-file save formats: the slot directory, campaign progress, screen mode, and each
//! slot part on its own.

#![no_main]

use libfuzzer_sys::fuzz_target;
use s2_formats::save::{
    ComputerPlayers, Control, Desktops, Directory, Mailboxes, MissionProgress, ScreenMode, Transport,
    WorldCampaignProgress,
};
use s2_fuzz::{check, debug};

fuzz_target!(|data: &[u8]| {
    check(Directory::parse(data), |dir| {
        for entry in dir.entries() {
            debug(&(entry.name(), entry.is_used(), entry.slot()));
        }
    });
    check(MissionProgress::parse(data), |m| debug(&m.unlocked()));
    check(WorldCampaignProgress::parse(data), |w| debug(&w));
    check(ScreenMode::parse(data), |s| debug(&(s.mode(), s.digit())));
    check(Transport::parse(data), |t| debug(&t));
    check(Control::parse(data), |c| {
        debug(&(c.layout(), c.game_time(), c.data().len()));
    });
    check(Desktops::parse(data), |d| debug(&d));
    check(Mailboxes::parse(data), |m| debug(&m));
    check(ComputerPlayers::parse(data), |c| debug(&c));
});
