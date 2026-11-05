// Returned errors are UTF-8, owned by Rust; copy before freeing.
char *qrc_test_initialize(void);
char *qrc_test_query(void);
void qrc_test_free_error(char *error);
