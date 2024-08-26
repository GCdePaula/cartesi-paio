session="cartesipaio"

# set up tmux
tmux start-server

# create a new tmux session with <NAME>
tmux new-session -d -s $session -n servers

# Select pane 0, set dir to <PROJECT NAME>
tmux selectp -t 0
tmux set -g pane-border-status top
tmux set -g pane-border-format "#{pane_index} #{pane_current_command}"
tmux send-keys "source ~/.bashrc" C-m
tmux send-keys "cd ../tripa" C-m
tmux send-keys "anvil" C-m
sleep 1

# Select pane 1
tmux splitw -h
tmux set -g pane-border-status top
tmux set -g pane-border-format "#{pane_index} #{pane_current_command}"
tmux send-keys "source ~/.bashrc" C-m
tmux send-keys "cd ../tripa" C-m
tmux send-keys "./fund-sequencer.sh" C-m
sleep 1
tmux send-keys "cargo run" C-m

# Select pane 2
tmux splitw -v
tmux set -g pane-border-status top
tmux set -g pane-border-format "#{pane_index} #{pane_current_command}"
tmux send-keys "source ~/.bashrc" C-m

# create a new window called <PROJECT NAME>
#tmux new-window -t $session:2 -n <PROJECT NAME>
#tmux send-keys "cd ~/path/to/project" C-m
#tmux send-keys "vim ." C-m

# create a new window called <PROJECT NAME>
#tmux new-window -t $session:3 -n <PROJECT NAME>
#tmux send-keys "cd ~/path/to/project" C-m
#tmux send-keys "vim ." C-m

# create a new window called <PROJECT NAME>
#tmux new-window -t $session:4 -n scratch

# return to main servers window
#tmux select-window -t $session:1

# Finished setup, attach to the tmux session!
tmux attach-session -t $session

