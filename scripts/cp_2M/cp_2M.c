#define _GNU_SOURCE     // MAP_ANONYMOUS
#include <stdio.h>      // perror(3), fprintf(3), stdout(3), stderr(3)
#include <stdlib.h>     // exit(3), malloc(3)
#include <string.h>     // memset(3)
#include <errno.h>      // errno(3), ENOENT
#include <unistd.h>     // stat(2), read(2), write(2), close(2), EXIT_*
#include <sys/types.h>  // open(2), stat(2)
#include <sys/stat.h>   // open(2), stat(2)
#include <fcntl.h>      // open(2)
#include <sys/mman.h>   // mmap(2), munmap(2)

#define TWO_MEBIBYTES	(1 << 21)

#define die(msg) \
	do { perror(msg); exit(EXIT_FAILURE); } while (0)

int main(int argc, char *argv[])
{
	struct stat st;
	int ret, fd_src, fd_dst;
	char *buf;

	if (3 != argc) {
		fprintf(stderr, "\nUsage:\n\t$ %s <src-file> <dst-file>\n\n",
				argv[0]);
		return EXIT_FAILURE;
	}

	// Make sure that the destination path does not already exist, to avoid
	// any unwanted overwrites.
	ret = stat(argv[2], &st);
	if (0 == ret) {
		fprintf(stderr, "File '%s' already exists!\n", argv[2]);
		return EXIT_FAILURE;
	} else if (-1 == ret && ENOENT != errno)
		die("stat (dst-file)");

	// Make sure that the source path exists.
	if (-1 == stat(argv[1], &st))
		die("stat (src-file)");

	if (-1 == (fd_src = open(argv[1], O_RDONLY)))
		die("open (src-file)");
	if (-1 == (fd_dst = open(argv[2], O_WRONLY | O_CREAT, st.st_mode)))
		die("open (dst-file)");

	fprintf(stdout, "'%s' --> '%s'\n", argv[1], argv[2]);

	buf = mmap(NULL, TWO_MEBIBYTES, PROT_READ | PROT_WRITE,
			MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (MAP_FAILED == buf)
		die("mmap");

	for (ssize_t nr, nw, size = st.st_size; size > 0; size -= nr) {
		nr = read(fd_src, buf, TWO_MEBIBYTES);
		if (-1 == nr)
			die("read");
		if (0 == nr)
			break;
		nw = write(fd_dst, buf, nr);
		if (nr != nw)
			die("write");
	}

	if (-1 == close(fd_src))
		perror("close (src-file)");
	if (-1 == close(fd_dst))
		perror("close (dst-file)");
	if (-1 == munmap(buf, TWO_MEBIBYTES))
		die("munmap");

	return EXIT_SUCCESS;
}

