# AutoFate

Autofate is a simple piece of software which is configured to run the fate suite of tests from FFmpeg for each commit from a repo in order to help find a commit which is failing to build. It is written in Rust so that it can be run on any platform and then report regressions to the fate servers, eventually allowing extension into other reporting mechanisms.

Sample config file is included in the form of config.yaml with support for various invocations.

For setup you need to have the environment setup such that you can run "configure" and "make fate" (installing relevant developer packages for your platform).
