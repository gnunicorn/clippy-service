# Clippy Service Changelog

## April 14th 2016

 - We know have an [automatic docker build](https://hub.docker.com/r/lightyear/clippy-service/) on every push for you (not necessarily always up to date though)
 - build docker image from travis and push it directly instead of building it with dokku
 - updated to latest firejail fixes a bunch of false-positives
 - minor TLS fixes

## Mar 24th 2016

 - ensure we are running nightly
 - Add version print of rustc and cargo to logs
 - Update docs

## Mar 9th 2016

 - Use improved Redis locking mechanism
 - Limit CPU and Execution time of background process
 - Minor fixes in rendering of copy-paste-code
 - add Contributors section.

## Mar 3rd 2016, 1.0-beta3

 - add emoji badges
 - improve Website
 - adding logo
 - add changelog

## Mar 2nd 2016, 1.0-beta2

 - Add Travis build, use nightli.es to build every day
 - Add Website

## Feb 29th 2016: 1.0-beta

 - run clippy within the firejail sandbox
 - always run clippy from local dependencies

## Feb 26th, 2016 : 1.0-alpha

 - First feature complete alpha version
  - render badges
  - run clippy on process
  - caching of old results
 - includes Vagrant + Docker for Deploy
