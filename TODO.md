# TODO List

## Implement search

Implement search on node entry name and folder name

## Cache EntryView

EntryData is loaded when EntryView is opened, without cache. A bit slow (1 sec).
Could either:
  - load everything at start. Slow at start but fast afterwards and allow deep search
  - cache once opened

## Implement changes view

Show git history

## Allow 3-pane view

Allow to separate tree_view for folder and leaf/entry
Dedicated entry pane
Should be configurable/optional

## Init repo

If ~/.password_store does not exists, show a view to create gitproject
- propose link to gitlab, github
- Also create pgp key if not existing ?

check pass init process

## Implement OTP

handle OTP password and fields

## Implement multiline

handle multiline custom field
