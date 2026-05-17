#!/bin/bash
# Activate virtual environment
source venv/bin/activate

# Set LibTorch variables
export LIBTORCH_USE_PYTORCH=1
export LIBTORCH_BYPASS_VERSION_CHECK=1

# Set Library Path for macOS (dynamically find torch lib)
export DYLD_LIBRARY_PATH=$(python -c "import torch; import os; print(os.path.join(os.path.dirname(torch.__file__), 'lib'))"):$DYLD_LIBRARY_PATH

# Run the game
cargo run
