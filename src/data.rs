#![allow(unused)]

use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

// Connector positions (hook, cable, ...) extracted via ModelViewer2.
// 1. Open Connector Tool
// 2. Select model connector name (name can be found in `D:\DCS World\CoreMods\tech\USS_Nimitz\
//    scripts\USS_Nimitz_RunwaysAndRoutes.lua`)
// 3. Read P position row as (z, y, x)

const NIMITZ: CarrierInfo = CarrierInfo {
    // CoreMods\tech\USS_Nimitz\scripts\USS_Nimitz_RunwaysAndRoutes.lua
    deck_angle: 9.1359,
    deck_altitude: 20.1494,
    cable1: (
        // POINT_TROS_01_01
        DVec3 {
            x: -17.622131,
            y: 20.201731,
            z: -112.129128,
        },
        // POINT_TROS_01_02
        DVec3 {
            x: 18.445099,
            y: 20.201729,
            z: -106.040421,
        },
    ),
    cable2: (
        // POINT_TROS_02_01
        DVec3 {
            x: -19.584789,
            y: 20.201731,
            z: -99.914261,
        },
        // POINT_TROS_02_02
        DVec3 {
            x: 16.519514,
            y: 20.201729,
            z: -93.864029,
        },
    ),
    cable3: (
        // POINT_TROS_03_01
        DVec3 {
            x: -21.578857,
            y: 20.201731,
            z: -87.524025,
        },
        // POINT_TROS_03_02
        DVec3 {
            x: 14.471450,
            y: 20.201731,
            z: -81.399986,
        },
    ),
    cable4: (
        // POINT_TROS_04_01
        DVec3 {
            x: -23.609934,
            y: 20.201731,
            z: -74.960480,
        },
        // POINT_TROS_04_02
        DVec3 {
            x: 12.444860,
            y: 20.201729,
            z: -68.854492,
        },
    ),
};

const FORRESTAL: CarrierInfo = CarrierInfo {
    // CoreMods\tech\USS_Nimitz\scripts\USS_Nimitz_RunwaysAndRoutes.lua
    deck_angle: 9.42,
    deck_altitude: 18.46,
    cable1: (
        // POINT_TROS_01_01
        DVec3 {
            x: -17.749493,
            y: 18.474249,
            z: -96.792412,
        },
        // POINT_TROS_01_02
        DVec3 {
            x: 17.089462,
            y: 18.474247,
            z: -90.162186,
        },
    ),
    cable2: (
        // POINT_TROS_02_01
        DVec3 {
            x: -19.516848,
            y: 18.475485,
            z: -87.192558,
        },
        // POINT_TROS_02_02
        DVec3 {
            x: 15.311986,
            y: 18.475483,
            z: -80.510368,
        },
    ),
    cable3: (
        // POINT_TROS_03_01
        DVec3 {
            x: -21.246920,
            y: 18.482229,
            z: -76.618980,
        },
        // POINT_TROS_03_02
        DVec3 {
            x: 13.582755,
            y: 18.482227,
            z: -69.941109,
        },
    ),
    cable4: (
        // POINT_TROS_04_01
        DVec3 {
            x: -23.128010,
            y: 18.491688,
            z: -66.396812,
        },
        // POINT_TROS_04_02
        DVec3 {
            x: 11.704433,
            y: 18.491686,
            z: -59.733154,
        },
    ),
};

static FA18C: AirplaneInfo = AirplaneInfo {
    hook: DVec3 {
        x: 0.0,
        y: -2.240897,
        z: -7.237348,
    },
    glide_slope: 3.5,
    plane_type: "FA18C",
};

static F14: AirplaneInfo = AirplaneInfo {
    hook: DVec3 {
        x: 0.0,
        y: -1.978941,
        z: -6.563727,
    },
    glide_slope: 3.5,
    plane_type: "F14",
};

static T45: AirplaneInfo = AirplaneInfo {
    hook: DVec3 {
        x: 0.0,
        y: -1.778766,
        z: -4.782536,
    },
    glide_slope: 3.5,
    plane_type: "T45",
};

#[derive(Debug)]
pub struct CarrierInfo {
    /// Counter-clockwise offset from BRC to FB in degrees.
    pub deck_angle: f64,
    // in meter
    pub deck_altitude: f64,
    /// Cable pendant positions (left, right) relative to the object' origin.
    pub cable1: (DVec3, DVec3),
    pub cable2: (DVec3, DVec3),
    pub cable3: (DVec3, DVec3),
    pub cable4: (DVec3, DVec3),
}

impl CarrierInfo {
    /// Calculate the offset from the origin where the optimal glide path hits the deck.
    pub fn optimal_landing_offset(&self, plane: &AirplaneInfo) -> DVec3 {
        // optimal hook touchdown point is halfway between the second and third cable
        // (according to NAVAIR 00-80T-104 4.2.8)
        let touchdown_at = (self.cable2.0 - self.cable3.1) / 2.0;
        let touchdown_at = self.cable3.1 + touchdown_at;

        let hook_offset = plane.hook.rotated_by(DRotor3::from_rotation_yz(
            plane.glide_slope.to_radians().neg(),
        ));

        touchdown_at - hook_offset
    }

    pub fn by_type(t: &str) -> Option<&'static Self> {
        match t {
            "CVN_71" | "CVN_72" | "CVN_73" | "CVN_75" | "Stennis" => Some(&NIMITZ),
            "Forrestal" => Some(&FORRESTAL),
            t => None,
        }
    }
}

#[derive(Debug)]
pub struct AirplaneInfo {
    /// Hook position relative to the object's origin.
    pub hook: DVec3,
    /// The optimal glide slope in degrees.
    pub glide_slope: f64,
    /// The type of aircraft used to select proper on speed color.
    pub plane_type: &'static str,
}

impl AirplaneInfo {
    pub fn by_type(t: &str) -> Option<&'static Self> {
        match t {
            "FA-18C_hornet" => Some(&FA18C),
            "F-14A-135-GR" | "F-14B" => Some(&F14),
            "T-45" => Some(&T45),
            t => None,
        }
    }
}
