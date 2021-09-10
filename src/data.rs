#![allow(unused)]

use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

// Connector positions (hook, cable, ...) extracted via ModelViewer2.
// 1. Open Connector Tool
// 2. Select model connector name (name can be found in `D:\DCS World\CoreMods\tech\USS_Nimitz\
//    scripts\USS_Nimitz_RunwaysAndRoutes.lua`)
// 3. Read P position row as (z, y, x)

pub const NIMITZ: CarrierInfo = CarrierInfo {
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

pub static FA18C: AirplaneInfo = AirplaneInfo {
    // hook_pooint
    hook: DVec3 {
        x: 0.0,
        y: -2.240897,
        z: -7.237348,
    },
};

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
    /// Calculate the offset from the origin where the optimal gliddepath hits the deck.
    pub fn optimal_landing_offset(&self, plane: &AirplaneInfo, glide_slope: f64) -> DVec3 {
        // optimal hook touchdown point is halfway between the second and third cable
        // (according to NAVAIR 00-80T-104 4.2.8)
        let touchdown_at = (self.cable2.0 - self.cable3.1) / 2.0;
        let touchdown_at = self.cable3.1 + touchdown_at;

        let hook_offset = plane
            .hook
            .rotated_by(DRotor3::from_rotation_yz(glide_slope.to_radians().neg()));

        touchdown_at - hook_offset
    }
}

pub struct AirplaneInfo {
    /// Hook position relative to the object's orign.
    hook: DVec3,
}
