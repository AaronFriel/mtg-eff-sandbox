mod effect_value;
mod interpreter;

use std::collections::HashMap;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use interpreter::Interpreter;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Game {
  pub life: usize,
  pub library: Vec<String>,
  pub hand: Vec<String>,
  pub graveyard: Vec<String>,

  pub replacement_effects: HashMap<String, Vec<serde_json::Value>>,
}

fn handle_replacement(
  int: &mut interpreter::Interpreter,
  replacement_key: &str,
) -> Option<<dyn DrawReplacement as ReplacementEffect>::Value> {
  let game = int.game();

  let alts = match game.replacement_effects.get(replacement_key) {
    Some(alts) => alts
      .iter()
      .filter_map(|s| serde_json::from_value::<Box<dyn DrawReplacement>>(s.clone()).ok())
      .filter(|eff| eff.check(game))
      .collect::<Vec<_>>(),
    None => Vec::new(),
  };
  if alts.len() == 1 {
    // Do the alternate effect
    return Some(alts[0].apply(int));
  }
  if !alts.is_empty() {
    todo!(); // Call back into the interpreter and ask the user interface to resolve, e.g.: user choice with player determined by APNAP
  }
  None
}

#[cfg(test)]
static GAIN_LIFE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Gain life effect, it does what it says on the tin. Effects are regular
/// looking functions.
///
/// We aren't addressing replacement effects here (intentionally), this is just
/// a prototype of a React hook like "useEffect" would look like for our use
/// case.
pub fn gain_life(amount: usize) -> impl FnOnce(&mut interpreter::Interpreter) -> String {
  move |int| {
    #[cfg(test)]
    GAIN_LIFE_CALL_COUNT.fetch_add(1, SeqCst);

    let mut g = int.game_mut();
    g.life += amount;

    format!("Added {amount} life")
  }
}

#[cfg(test)]
static DRAW_CARD_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Draw a single card effect.
pub fn draw_card(int: &mut Interpreter) -> Result<String, String> {
  #[cfg(test)]
  DRAW_CARD_CALL_COUNT.fetch_add(1, SeqCst);

  // Query game state for replacement effects:
  if let Some(value) = handle_replacement(int, "DRAW") {
    return value;
  }

  let game = int.game_mut();

  if let Some(card) = game.library.pop() {
    let message = format!("Drew {card}");
    game.hand.push(card);
    Ok(message)
  } else {
    Err("Drew from empty library! 💀".to_string())
  }
}

trait ReplacementEffect {
  type Value;

  fn apply(&self, int: &mut interpreter::Interpreter) -> Self::Value;
  fn check(&self, game: &Game) -> bool;
}

#[typetag::serde]
trait DrawReplacement: ReplacementEffect<Value = Result<String, String>> {}

#[derive(Serialize, Deserialize)]
struct RandomDiscardReplacement;

impl ReplacementEffect for RandomDiscardReplacement {
  type Value = Result<String, String>;

  fn apply(&self, int: &mut interpreter::Interpreter) -> Self::Value {
    let game = int.game_mut();

    // We would want to run an effect against an RNG, which would be part of the
    // "interface" of the interpreter and thus the interpreter would need a seed
    // for determinism.

    // Lacking that for example's sake, we'll just discard the last card:
    let discard = game.hand.pop().unwrap();

    // Replacement effects must honor the interface, e.g.: a "draw 2" is actually
    // "draw; draw", and "mill 4" is also a repeated effect.
    //
    // In a worked example, we'd be working with object IDs, not strings, and that
    // way we could handle replacement effects and interactions like Gyruda and
    // a replacement effect like Rest in Peace. Relevant effects:
    //
    // Gyruda: When Gyruda enters the battlefield, each player mills four cards. Put
    // a creature card with an even mana value from among the milled cards onto
    // the battlefield under your control.
    //
    // Rest in peace: If a card or token would be put into a graveyard from
    // anywhere, exile it instead.
    //
    // Even if Rest in Peace is in play, the replacement effect which moves the
    // cards to the exile zone has the same "signature" as mill, which moves
    // them to graveyard. Thus we can follow the object ID and Gyruda's effect
    // resolves, the word "milled" in "among the milled cards" is generalized to
    // whatever the replacement effect does.
    let message = format!("Discarded {}", discard);
    game.graveyard.push(discard);

    Ok(message)
  }

  fn check(&self, game: &Game) -> bool {
    !game.hand.is_empty()
  }
}

#[typetag::serde]
impl DrawReplacement for RandomDiscardReplacement {}

pub fn replace_draw_with_discard(int: &mut Interpreter) {
  let game = int.game_mut();

  let existing = game
    .replacement_effects
    .entry("DRAW".to_string())
    .or_default();

  let eff = &RandomDiscardReplacement as &dyn DrawReplacement;
  let eff = serde_json::to_value(eff).unwrap();
  existing.push(eff);
}

/// Draw multiple cards. Each one calls the draw card effect.
pub fn draw_cards(
  count: usize,
) -> impl FnOnce(&mut interpreter::Interpreter) -> Result<Vec<String>, String> {
  move |int| {
    let mut results = Vec::new();
    for _ in 1..=count {
      results.push(int.apply(draw_card)?);
    }

    Ok(results)
  }
}

#[cfg(test)]
mod test {
  use insta::{assert_json_snapshot, assert_yaml_snapshot};

  use super::*;
  use crate::interpreter::Interpreter;
  #[test]
  fn it_works() {
    // In this test we'll create a mock game state with two cards in the library,
    // none in hand, none in graveyard.
    //
    // We'll then simulate a game - we could do this incrementally or all at once!

    let mut g = Game {
      life: 20,
      library: vec!["Mox Tombstone".to_string(), "Mox Awesome".to_string()],
      hand: Vec::new(),
      graveyard: Vec::new(),
      replacement_effects: HashMap::new(),
    };

    let mut interpreter = Interpreter {
      game: &mut g,
      effects: Vec::new(),
      position: 0,
    };

    // In our first turn we draw a card, do nothing, and we return some state just
    // to prove that we can do so.
    let turn_one = |int: &mut Interpreter| {
      // Draw a single card
      let draw_result = int.apply(draw_card);

      assert_json_snapshot!(draw_result.unwrap(), @r###""Drew Mox Awesome""###);

      42
    };

    // In our second turn we draw, play a card that has a static ability - a
    // replacement effect that replaces draws with discarding.
    let turn_two = |int: &mut Interpreter| {
      // Use a helper method which runs a loop and draws multiple cards (each which
      // has replacement effects applied!)
      let draw_result = int.apply(draw_cards(1));

      assert_json_snapshot!(draw_result.unwrap()[0], @r###""Drew Mox Tombstone""###);

      // "Play" a card (we're skipping many steps) but, more or less, adding a
      // replacement effect
      int.apply(replace_draw_with_discard);

      69
    };

    // In our third turn we draw (which discards due to replacement effect) and
    // observe that we obtained that result. We also gain some life.
    let turn_three = |int: &mut Interpreter| {
      // Again run our "draw cards" loop with N=1, but this time expecting a different
      // result:
      let draw_result = int.apply(draw_cards(1));

      assert_json_snapshot!(draw_result.unwrap()[0], @r###""Discarded Mox Tombstone""###);

      // Gain some life:

      int.apply(gain_life(5));
    };

    // We'll use this later to verify that we can run the game incrementally or all
    // at once:
    let whole_game = |int: &mut Interpreter| {
      int.apply(turn_one);
      int.apply(turn_two);
      int.apply(turn_three);
    };

    // Start of game:
    assert_yaml_snapshot!(interpreter.game(), @r###"
    ---
    life: 20
    library:
      - Mox Tombstone
      - Mox Awesome
    hand: []
    graveyard: []
    replacement_effects: {}
    "###);

    interpreter.apply(turn_one);

    // Post turn one:
    assert_yaml_snapshot!(interpreter.game(), @r###"
    ---
    life: 20
    library:
      - Mox Tombstone
    hand:
      - Mox Awesome
    graveyard: []
    replacement_effects: {}
    "###);

    interpreter.apply(turn_two);

    // Post turn two:
    assert_yaml_snapshot!(interpreter.game(), @r###"
    ---
    life: 20
    library: []
    hand:
      - Mox Awesome
      - Mox Tombstone
    graveyard: []
    replacement_effects:
      DRAW:
        - RandomDiscardReplacement: ~
    "###);

    interpreter.apply(turn_three);

    // Post turn three:
    assert_yaml_snapshot!(interpreter.game(), @r###"
    ---
    life: 25
    library: []
    hand:
      - Mox Awesome
    graveyard:
      - Mox Tombstone
    replacement_effects:
      DRAW:
        - RandomDiscardReplacement: ~
    "###);

    let initial_snapshot = serde_json::to_value(&interpreter).unwrap();
    assert_eq!(GAIN_LIFE_CALL_COUNT.load(SeqCst), 1);
    assert_eq!(DRAW_CARD_CALL_COUNT.load(SeqCst), 3);

    // Re-run the interpreter, but re-use all existing effects. This won't actually
    // call any of the functions, but each effect's _result_ will be returned
    // from "apply" functions. Since all of these are deterministic, we can rapidly
    // "replay" the game up to the current decision point.

    // Even better, as effects are trees, we can represent the game as a series of
    // arbitrarily high level effects to obtain performance improvements or to
    // "skip ahead", e.g.: skip to the current player's turn and run the game
    // forward from that point.
    let effects = interpreter.effects;

    let mut interpreter = Interpreter {
      game: &mut g,
      // Re-use prior effects to prove idempotency.
      effects,
      position: 0,
    };

    whole_game(&mut interpreter);
    assert_eq!(GAIN_LIFE_CALL_COUNT.load(SeqCst), 1);
    assert_eq!(DRAW_CARD_CALL_COUNT.load(SeqCst), 3);

    let final_snapshot = serde_json::to_value(&interpreter).unwrap();

    assert_eq!(initial_snapshot, final_snapshot);
    assert_yaml_snapshot!(interpreter, @r###"
    ---
    game:
      life: 25
      library: []
      hand:
        - Mox Awesome
      graveyard:
        - Mox Tombstone
      replacement_effects:
        DRAW:
          - RandomDiscardReplacement: ~
    effects:
      - result: 42
        children:
          - result:
              Ok: Drew Mox Awesome
            children: []
      - result: 69
        children:
          - result:
              Ok:
                - Drew Mox Tombstone
            children:
              - result:
                  Ok: Drew Mox Tombstone
                children: []
          - result: ~
            children: []
      - result: ~
        children:
          - result:
              Ok:
                - Discarded Mox Tombstone
            children:
              - result:
                  Ok: Discarded Mox Tombstone
                children: []
          - result: Added 5 life
            children: []
    position: 3
    "###);
  }
}
