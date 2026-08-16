#ifndef LAWKITT_ANYDOC_H
#define LAWKITT_ANYDOC_H

#include <stddef.h>
#include <stdint.h>

#define LAWKITT_ANYDOC_OUTCOME_OK 0
#define LAWKITT_ANYDOC_OUTCOME_UNSUPPORTED 2
#define LAWKITT_ANYDOC_OUTCOME_MALFORMED 3
#define LAWKITT_ANYDOC_OUTCOME_ENCRYPTED 4
#define LAWKITT_ANYDOC_OUTCOME_RESOURCE_LIMIT 5
#define LAWKITT_ANYDOC_OUTCOME_MISSING_PART 6
#define LAWKITT_ANYDOC_OUTCOME_IO 7
#define LAWKITT_ANYDOC_OUTCOME_INVALID_ARGUMENT 8
#define LAWKITT_ANYDOC_OUTCOME_SCANNED_ONLY 9
#define LAWKITT_ANYDOC_OUTCOME_PANIC 10

#define LAWKITT_ANYDOC_FORMAT_PDF 1
#define LAWKITT_ANYDOC_FORMAT_DOCX 2

int lawkitt_anydoc_to_markdown(
    const uint8_t *bytes,
    size_t len,
    int format_tag,
    uint8_t **out_ptr,
    size_t *out_len
);

void lawkitt_anydoc_buffer_free(uint8_t *buffer, size_t len);

#endif
