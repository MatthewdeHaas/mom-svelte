-- seed_recipes.sql
INSERT INTO companies (name, contact_email, active) VALUES
  ('Brightstone Partners', null, true),
  ('Oakfield Group',       null, true),
  ('Meridian Co',          null, true),
  ('Crestview Solutions',  null, true),
  ('Harlow & Associates',  null, true)
ON CONFLICT (name) DO NOTHING;

INSERT INTO recipes (name, active) VALUES
  ('Avocado Toast & Eggs',   true),
  ('Greek Yoghurt Parfait',  true),
  ('Granola & Berry Bowl',   true),
  ('Smoked Salmon Bagel',    true),
  ('Grilled Chicken Caesar', true),
  ('Roasted Tomato Soup',    true),
  ('Thai Green Curry',       true),
  ('Lamb Tagine',            true),
  ('Salmon & Dill Risotto',  true),
  ('Beef & Vegetable Stew',  true)
ON CONFLICT (name) DO NOTHING;
