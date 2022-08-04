use serde::{de::DeserializeOwned, Serialize};

use super::{
  effect_value::{EffectTree, EffectValue},
  Game,
};

/// This simple interpreter acts a lot like an iterator over a tree. Every time
/// we call "apply" it recurses into the effect tree and creates a child
/// iterator to pass to the function.
///
/// The child iterator tracks its position in the list of effects. If the
/// effect's value is recorded in our list, then we return the value and skip
/// applying the "effect".
///
/// If the effect's value is not in our list, i.e.: we've reached the end of the
/// list, we run the effect and give it a mutable game.
///
/// This memoizes a tree of function calls, which just like the rules for React
/// hooks, must be a deterministic sequence (or tree!).
#[derive(Serialize)]
pub struct Interpreter<'a> {
  pub(crate) game: &'a mut Game,
  pub(crate) effects: Vec<EffectTree>,
  pub(crate) position: usize,
}

impl<'a> Interpreter<'a> {
  pub(crate) fn apply<T, F>(&mut self, f: F) -> T
  where
    F: for<'x> FnOnce(&mut Interpreter<'x>) -> T,
    T: Serialize + DeserializeOwned + 'static,
    Self: Sized,
  {
    if let Some(dec) = self.effects.get(self.position) {
      self.position += 1;
      let result: T = dec.result.get().unwrap();
      return result;
    }
    self.position += 1;

    // This is annoying - we need a SimpleInterpreter<'x> - with the EXACT lifetime
    // 'x but lifetime rules mean any we construct in this function have a
    // lifetime 'y < 'x

    // So we safe our own state, then restore it afterward. Like I said, silly! I'm
    // sure there's a way to type this, but I think the fact that our "f" is a
    // "impl FnOnce(&mut Self)", where Self is _our own type_, i.e.: with <'x>, is
    // the problem.
    //
    // Is there a way to write a trait such that a method can take an argument that
    // is a function, where the function's argument is a subtype by lifetime?
    // Probably. Not bothering now.
    let mut sub_int = Interpreter {
      game: self.game,
      effects: Vec::new(),
      position: 0,
    };

    let outcome = f(&mut sub_int);

    self.effects.push(EffectTree {
      result: EffectValue::new(&outcome).unwrap(),
      children: sub_int.effects,
    });

    outcome
  }

  pub(crate) fn game(&self) -> &Game {
    self.game
  }

  pub(crate) fn game_mut(&mut self) -> &mut Game {
    self.game
  }
}
