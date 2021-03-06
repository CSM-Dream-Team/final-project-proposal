use std::io::{Read, Write};
use std::boxed::FnBox;

use serde::{self, Serialize, Deserialize};
use serde_json::{Deserializer, Serializer, Error as JsonError};
use serde_json::de::IoRead as JsonRead;

use nalgebra::{self as na, Vector3, Isometry3, Translation3, UnitQuaternion};
use ncollide::shape::{ShapeHandle, Compound, Cuboid, Cylinder};
use nphysics3d::object::RigidBody;

// Flight
use flight::{UberMesh, Error};

// GFX
use gfx;
use app::App;

use common::{open_object_directory, Common, CommonReply, Meta};
use common::gurus::interact::GrabbablePhysicsState;

pub struct LetsGetPhysical<R: gfx::Resources> {
    mjolnir: UberMesh<R>,
    grabbable_state: GrabbablePhysicsState,
}

#[derive(Serialize, Deserialize)]
pub struct LetsGetPhysicalState {
    location: Isometry3<f32>,
}

fn spawn_mjolnir() -> GrabbablePhysicsState {
    let shapes = vec![(Isometry3::new(Vector3::new(0., -3., 0.) * 0.08, na::zero()),
                       ShapeHandle::new(Cuboid::new(Vector3::new(2.0, 1.5, 1.5) * 0.08))),
                      (Isometry3::new(Vector3::new(0., 1.25, 0.) * 0.08, na::zero()),
                       ShapeHandle::new(Cylinder::new(3.25 * 0.08, 0.5 * 0.08)))];
    let compound = Compound::new(shapes);

    let mut mjolnir_body = RigidBody::new_dynamic(compound, 2330., 0.35, 0.47);
    let location = Isometry3::new(Vector3::new(0., 2.5, 0.), na::zero());
    mjolnir_body.set_transformation(location);

    GrabbablePhysicsState::new_free(mjolnir_body)
}

impl<R: gfx::Resources> LetsGetPhysical<R> {
    pub fn new<F: gfx::Factory<R>>(factory: &mut F) -> Result<Self, Error> {
        Ok(LetsGetPhysical {
            mjolnir: open_object_directory(factory, "assets/hammer/")?,
            grabbable_state: spawn_mjolnir(),
        })
    }
}

impl<R: gfx::Resources + 'static, C: gfx::CommandBuffer<R> + 'static, W: Write, Re: Read> App<R, C, W, Re>
    for LetsGetPhysical<R> {
    fn se_state(&self,
                serializer: &mut Serializer<W>, _: &mut Meta)
                -> Result<<&mut Serializer<W> as serde::Serializer>::Ok, JsonError> {
        let state = LetsGetPhysicalState {
            location: *self.grabbable_state.body.position(),
        };
        state.serialize(serializer)
    }

    fn de_state(&mut self, deserializer: &mut Deserializer<JsonRead<Re>>, _: &mut Meta) -> Result<(), JsonError> {
        let state = LetsGetPhysicalState::deserialize(deserializer)?;
        self.grabbable_state.body.set_transformation(state.location);
        Ok(())
    }

    fn update<'b>(&'b mut self,
                  common: &mut Common<R, C>)
                  -> Box<FnBox(&mut CommonReply<R, C>) + 'b> {
        // Reset if you throw it off the platform.
        // TODO: this doesn't work with alternate gravity
        if self.grabbable_state.body.position().translation.vector.y < -10. {
            self.grabbable_state = spawn_mjolnir();
        }

        let gp = self.grabbable_state.update(&mut common.gurus.interact,
                                             &mut common.gurus.physics,
                                             Isometry3::from_parts(
                                                 Translation3::new(0., 0., -0.2),
                                                 UnitQuaternion::rotation_between(
                                                     &Vector3::new(0., -1., 0.),
                                                     &Vector3::new(0., 0., -1.),
                                                 ).unwrap(),
                                             ),
                                             0.2 / common.meta.physics_speed);

        let mjolnir = &self.mjolnir;
        Box::new(move |r: &mut CommonReply<R, C>| {
            let pos = gp(r);
            r.painters.uber.draw(&mut r.draw_params, na::convert(pos), mjolnir);
        })
    }
}
