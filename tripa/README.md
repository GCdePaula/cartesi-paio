Dependencies
============

Install dependencies:

    sudo apt install libssl-dev

Install `foundry`, including `avail` and `cast`.

Running
=======

For useful scripts, install `jaq`:

    cargo install --locked jaq
    cargo install tomlq

Run anvil:

    anvil

Fund account:

    ./fund_sequencer

Run `tripa`:

    cargo run

try to change privacy_file_unique_origin to false in about:config, restart firefox and see if this can make a difference (please note that this makes you vulnerable to the described security problem though). 

Testing
=======

Then:

    cargo test
