CFLAGS = -Wall -Wextra -Werror -O2 -std=c11 -pedantic
BIN = cp_2M

$(BIN): cp_2M.c
	$(CC) $(CFLAGS) $< -o $@
	strip -s $@

.PHONY: clean
clean:
	$(RM) $(BIN)

