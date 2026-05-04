// BUILD WITH: gcc -Wall -Werror -o obj_test.elf obj_test.c
#include <stdio.h>

int main(int argc, char** argv) {
	printf("I got %d arguments", argc);
	return 0;
}

