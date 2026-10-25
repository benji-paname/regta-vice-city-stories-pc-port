//! First unarmed combat controller. Poses/durations are native; contact timing,
//! damage and AI decisions remain provisional until their original data is mapped.
use crate::character::{Character, Pose};
use crate::collision::{ActorQuery, CollisionQuery};
use crate::iso::Result;

pub const REACH: f32 = 1.65;
pub const DAMAGE: u32 = 12;
const ATTACKS: [&str; 2] = ["Fight_Ljab", "Fight_Rcross"];
const REACTIONS: [&str; 2] = ["Hit_L_hook", "Hit_R_hook"];

pub struct Profile {
    attacks: [f32; 2],
    reactions: [f32; 2],
    fall: f32,
}
impl Profile {
    #[cfg(test)]
    pub(crate) fn fixture() -> Self {
        Self {
            attacks: [0.6, 19. / 30.],
            reactions: [14. / 30.; 2],
            fall: 0.8,
        }
    }
    pub fn load(actor: &Character) -> Result<Self> {
        let duration = |name| -> Result<f32> {
            actor
                .duration(name)
                .ok_or_else(|| format!("Missing combat clip {name}").into())
        };
        Ok(Self {
            attacks: [duration(ATTACKS[0])?, duration(ATTACKS[1])?],
            reactions: [duration(REACTIONS[0])?, duration(REACTIONS[1])?],
            fall: duration("KO_skid_back")?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Free,
    Guard,
    Attack(usize),
    Hit(usize),
    Down,
}
pub struct Fighter {
    pub health: u32,
    pub armor: u32,
    action: Action,
    clock: f32,
    next: usize,
    cooldown: f32,
    strike: bool,
    pub hits: u32,
    pub hits_taken: u32,
}
impl Default for Fighter {
    fn default() -> Self {
        Self {
            health: 100,
            armor: 0,
            action: Action::Free,
            clock: 0.,
            next: 0,
            cooldown: 0.,
            strike: false,
            hits: 0,
            hits_taken: 0,
        }
    }
}
impl Fighter {
    pub fn alive(&self) -> bool {
        self.health > 0
    }
    pub fn busy(&self) -> bool {
        matches!(
            self.action,
            Action::Attack(_) | Action::Hit(_) | Action::Down
        )
    }
    pub fn stance(&mut self, guard: bool) {
        if !self.busy() {
            let action = if guard { Action::Guard } else { Action::Free };
            if action != self.action {
                self.action = action;
                self.clock = 0.;
            }
        }
    }
    pub fn attack(&mut self) -> bool {
        if self.busy() || !self.alive() || self.cooldown > 0. {
            return false;
        }
        self.action = Action::Attack(self.next);
        self.next = 1 - self.next;
        self.clock = 0.;
        self.strike = false;
        true
    }
    pub fn tick(&mut self, profile: &Profile, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0. {
            return false;
        }
        let dt = dt.min(0.25);
        self.cooldown = (self.cooldown - dt).max(0.);
        self.clock += dt;
        let mut hit = false;
        match self.action {
            Action::Attack(i) => {
                if !self.strike && self.clock >= profile.attacks[i] * 0.42 {
                    self.strike = true;
                    hit = true;
                }
                if self.clock >= profile.attacks[i] {
                    self.action = Action::Guard;
                    self.clock = 0.;
                    self.cooldown = 0.12;
                }
            }
            Action::Hit(i) if self.clock >= profile.reactions[i] => {
                self.action = Action::Guard;
                self.clock = 0.;
            }
            Action::Down => {
                self.clock = self.clock.min(profile.fall);
            }
            _ => {}
        }
        hit
    }
    pub fn damage(&mut self, amount: u32, side: usize) {
        if !self.alive() || amount == 0 {
            return;
        }
        let absorbed = self.armor.min(amount);
        self.armor -= absorbed;
        self.health = self.health.saturating_sub(amount - absorbed);
        self.action = if self.alive() {
            Action::Hit(side % 2)
        } else {
            Action::Down
        };
        self.clock = 0.;
        self.strike = true;
        self.hits_taken += 1;
    }
    pub fn cancel(&mut self) {
        if self.alive() {
            self.action = Action::Free;
            self.clock = 0.;
            self.strike = false;
        }
    }
    pub fn pose(&self) -> Option<Pose> {
        let (clip, looping) = match self.action {
            Action::Free => return None,
            Action::Guard => ("FIGHTIDLE", true),
            Action::Attack(i) => (ATTACKS[i], false),
            Action::Hit(i) => (REACTIONS[i], false),
            Action::Down => ("KO_skid_back", false),
        };
        Some(Pose {
            clip,
            time: self.clock,
            looping,
            root_height: true,
        })
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"health":self.health,"armor":self.armor,"action":format!("{:?}",self.action),"time":self.clock,"hits":self.hits,"hits_taken":self.hits_taken})
    }
}

/// Tests range, facing, floor and line of sight using solid world/vehicle geometry.
/// Candidates exclude the attacker and downed bodies. One strike selects one victim.
pub fn target(
    world: &impl CollisionQuery,
    origin: [f32; 3],
    heading: f32,
    candidates: &[[f32; 3]],
    range: f32,
) -> Option<usize> {
    let (s, c) = heading.sin_cos();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let dx = p[0] - origin[0];
            let dy = p[1] - origin[1];
            let length = dx.hypot(dy);
            if !length.is_finite()
                || length > range
                || (p[2] - origin[2]).abs() > 0.8
                || (length > 0.01 && (dx * s + dy * c) / length < 0.45)
            {
                return None;
            }
            let a = [origin[0], origin[1], origin[2] + 1.05];
            let delta = [dx, dy, p[2] - origin[2]];
            let distance = (delta.iter().map(|x| x * x).sum::<f32>()).sqrt();
            if distance > 0.01 && ActorQuery(world).raycast(a, delta, distance).is_some() {
                return None;
            }
            Some((i, length))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{CollisionWorld, Triangle};
    fn profile() -> Profile {
        Profile {
            attacks: [0.6, 0.6333333],
            reactions: [0.4666667; 2],
            fall: 0.8,
        }
    }
    #[test]
    fn swing_hits_once_and_does_not_restart_when_held() {
        let mut f = Fighter::default();
        assert!(f.attack());
        let mut hits = 0;
        for _ in 0..80 {
            assert!(!f.attack());
            hits += f.tick(&profile(), 1. / 120.) as usize;
        }
        assert_eq!(hits, 1);
        for _ in 0..20 {
            f.tick(&profile(), 1. / 120.);
        }
        assert!(f.attack());
        assert_eq!(f.pose().unwrap().clip, "Fight_Rcross");
    }
    #[test]
    fn armor_damage_death_and_interruption_are_saturating() {
        let mut f = Fighter::default();
        f.armor = 5;
        f.attack();
        f.damage(12, 0);
        assert_eq!((f.health, f.armor), (93, 0));
        assert!(!f.tick(&profile(), 0.25));
        f.damage(u32::MAX, 1);
        assert!(!f.alive());
        assert!(!f.attack());
        for _ in 0..100 {
            assert!(!f.tick(&profile(), 0.1));
        }
        assert_eq!(f.pose().unwrap().time, 0.8);
        f.cancel();
        assert!(!f.alive());
        assert!(f.busy());
    }
    #[test]
    fn strikes_reject_back_targets_other_floors_range_and_walls() {
        let empty = CollisionWorld::from_triangles(vec![]);
        let ps = [
            [0., -1., 0.],
            [0., 1., 3.],
            [0., 4., 0.],
            [0., 1.3, 0.],
            [0., 1., 0.],
        ];
        assert_eq!(target(&empty, [0.; 3], 0., &ps, REACH), Some(4));
        let wall = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-5., 0.5, -5.], [5., 0.5, -5.], [5., 0.5, 5.]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-5., 0.5, -5.], [5., 0.5, 5.], [-5., 0.5, 5.]],
                surface: 0,
                model_id: 1,
            },
        ]);
        assert_eq!(target(&wall, [0.; 3], 0., &ps, REACH), None);
        assert!(target(&empty, [0.; 3], 0., &[[f32::NAN, 0., 0.]], REACH).is_none());
    }
}
