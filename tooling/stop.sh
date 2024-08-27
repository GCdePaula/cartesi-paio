session="cartesipaio"

for t in 0 1 2 3;
do
  tmux selectp -t 0
  tmux send-keys -t 0 C-c
  sleep 1
  tmux kill-pane -t 0
done;
