#include <time.h>
#include <stdlib.h>
#include <unistd.h>
#include <math.h>
#include <regex.h>
#include <string.h>
#include <stdbool.h>
#include <curl/curl.h>
#include <ctype.h>
#include <zip.h>
#include <sys/stat.h>
#include <errno.h>
#include "gui_gtk.h"

double get_time()
{
    return (double)time(NULL);
}

int system_f(char* argument)
{
    return system(argument);
}

int get_random(int min, int max)
{
    static int seeded = 0;
    static unsigned int high_entropy_seed = 0;
    
    if (!seeded)
    {
        struct timespec ts;
        clock_gettime(CLOCK_MONOTONIC, &ts);
        
        // Gather entropy from multiple sources
        unsigned int seed = 0;
        
        // 1. Time entropy (seconds + nanoseconds)
        seed ^= ts.tv_sec;
        seed ^= ts.tv_nsec;
        seed ^= ts.tv_nsec << 13;
        seed ^= ts.tv_nsec >> 17;
        
        // 2. Process ID entropy
        seed ^= (unsigned int)getpid();
        seed ^= (unsigned int)getpid() << 9;
        seed ^= (unsigned int)getppid();
        
        // 3. CPU clock entropy
        seed ^= (unsigned int)clock();
        seed ^= (unsigned int)clock() << 11;
        
        // 4. Stack address entropy (different for each run)
        int stack_var = 0;
        seed ^= (unsigned int)(unsigned long)&stack_var;
        seed ^= (unsigned int)(unsigned long)&seed;
        
        // 5. Function address entropy
        seed ^= (unsigned int)(unsigned long)get_random;
        
        // 6. Time in milliseconds with high precision
        struct timeval tv;
        gettimeofday(&tv, NULL);
        seed ^= tv.tv_sec;
        seed ^= tv.tv_usec;
        seed ^= tv.tv_usec << 17;
        
        // 7. Additional entropy from rand() if available (but not seeded yet)
        // Use a simple counter based on time
        static int counter = 0;
        seed ^= (unsigned int)time(NULL) + (counter++);
        
        // 8. XOR with a high-quality hash of the seed itself
        // Thomas Wang's 32-bit integer hash for better distribution
        seed = (seed ^ 61) ^ (seed >> 16);
        seed = seed + (seed << 3);
        seed = seed ^ (seed >> 4);
        seed = seed * 0x27d4eb2d;
        seed = seed ^ (seed >> 15);
        
        // 9. Add entropy from the current time again (in case of rapid calls)
        clock_gettime(CLOCK_MONOTONIC, &ts);
        seed ^= ts.tv_nsec;
        seed ^= ts.tv_sec << 19;
        
        // 10. Final mixing with prime numbers
        seed ^= 0x9e3779b9;  // Golden ratio constant
        seed ^= seed >> 11;
        seed ^= seed << 7;
        seed ^= seed >> 19;
        
        // Store for later use
        high_entropy_seed = seed;
        
        // Initialize srand with our high-entropy seed
        srand(seed);
        seeded = 1;
        
        // Aggressive burn-in to eliminate any patterns
        for (int i = 0; i < 100; i++)
        {
            rand();
            // Mix in more entropy during burn-in
            if (i % 10 == 0)
            {
                clock_gettime(CLOCK_MONOTONIC, &ts);
                srand(rand() ^ ts.tv_nsec);
            }
        }
        
        // One final reseed with accumulated entropy
        srand(rand() ^ high_entropy_seed ^ (unsigned int)clock());
    }
    
    // Generate random number with additional whitening
    int r = rand();
    
    // XOR with stored high-entropy seed for extra randomness
    r ^= high_entropy_seed;
    
    // Rotate bits for better distribution
    r = ((r >> 16) ^ r) * 0x85ebca6b;
    r = ((r >> 13) ^ r) * 0xc2b2ae35;
    r = (r >> 16) ^ r;
    
    // Ensure positive
    if (r < 0) r = -r;
    
    // Calculate range
    int range = max - min + 1;
    
    // Use rejection sampling to avoid modulo bias
    int result;
    do
    {
        r = rand();
        r ^= high_entropy_seed;
        r = ((r >> 16) ^ r) * 0x85ebca6b;
        r = ((r >> 13) ^ r) * 0xc2b2ae35;
        r = (r >> 16) ^ r;
        if (r < 0) r = -r;
        result = min + (r % range);
        
        // For small ranges, reduce bias by retrying if needed
        if (range < 10000)
        {
            int max_valid = (RAND_MAX / range) * range;
            if (r > max_valid) continue;
        }
        break;
    } while (1);
    
    // Update entropy for next
    high_entropy_seed ^= (unsigned int)result;
    high_entropy_seed ^= (unsigned int)clock();
    
    return result;
}

int sleep_seconds(int seconds)
{
    #ifdef _WIN32
        Sleep(seconds * 1000); 
        return 0;
    #else
        return sleep(seconds); 
    #endif
}

void exit_program(int flag)
{
    exit(flag);
}

char* date(const char* format)
{
    time_t rawtime;
    struct tm* timeinfo;
    
    time(&rawtime);
    timeinfo = localtime(&rawtime);
    
    if (timeinfo == NULL) return NULL;
    
    // Allocate fresh buffer each time
    char* result = calloc(1024, sizeof(char));
    if (result == NULL) return NULL;
    
    char temp[100];
    int pos = 0;
    int i = 0;
    
    // Clear buffer
    result[0] = '\0';
    
    while (format[i] != '\0' && pos < 900)
    {
        // Handle escape sequences
        if (format[i] == '\\')
        {
            i++;
            if (format[i] != '\0')
            {
                result[pos++] = format[i];
                i++;
            }
            continue;
        }
        
        // Format specifiers
        switch (format[i])
        {
            // ========== DAY ==========
            case 'd': // Day with leading zeros
                sprintf(temp, "%02d", timeinfo->tm_mday);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 'j': // Day without leading zeros
                sprintf(temp, "%d", timeinfo->tm_mday);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 'D': // Short day name
                {
                    int wday = timeinfo->tm_wday;
                    if (wday == 0) wday = 7;
                    const char* days[] = {"Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"};
                    strcpy(result + pos, days[wday - 1]);
                    pos += 3;
                }
                break;
                
            case 'l': // Full day name
                {
                    int wday = timeinfo->tm_wday;
                    if (wday == 0) wday = 7;
                    const char* days[] = {"Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"};
                    strcpy(result + pos, days[wday - 1]);
                    pos += strlen(days[wday - 1]);
                }
                break;
                
            case 'N': // ISO day (1=Mon, 7=Sun)
                {
                    int wday = timeinfo->tm_wday;
                    if (wday == 0) wday = 7;
                    sprintf(temp, "%d", wday);
                    strcpy(result + pos, temp);
                    pos += strlen(temp);
                }
                break;
                
            case 'w': // Day of week (0=Sun)
                sprintf(temp, "%d", timeinfo->tm_wday);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 'z': // Day of year
                sprintf(temp, "%d", timeinfo->tm_yday);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            // ========== WEEK ==========
            case 'W': // ISO week number
                {
                    int week = (timeinfo->tm_yday - timeinfo->tm_wday + 10) / 7;
                    if (week < 1) week = 1;
                    if (week > 52) week = 52;
                    sprintf(temp, "%02d", week);
                    strcpy(result + pos, temp);
                    pos += 2;
                }
                break;
                
            // ========== MONTH ==========
            case 'F': // Full month name
                {
                    const char* months[] = {"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"};
                    strcpy(result + pos, months[timeinfo->tm_mon]);
                    pos += strlen(months[timeinfo->tm_mon]);
                }
                break;
                
            case 'm': // Month with leading zeros
                sprintf(temp, "%02d", timeinfo->tm_mon + 1);
                strcpy(result + pos, temp);
                pos += 2;
                break;
                
            case 'M': // Short month name
                {
                    const char* months[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
                    strcpy(result + pos, months[timeinfo->tm_mon]);
                    pos += 3;
                }
                break;
                
            case 'n': // Month without leading zeros
                sprintf(temp, "%d", timeinfo->tm_mon + 1);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 't': // Days in month
                {
                    int days_in_month[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
                    int year = timeinfo->tm_year + 1900;
                    int leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
                    if (timeinfo->tm_mon == 1 && leap) {
                        strcpy(result + pos, "29");
                        pos += 2;
                    } else {
                        sprintf(temp, "%d", days_in_month[timeinfo->tm_mon]);
                        strcpy(result + pos, temp);
                        pos += strlen(temp);
                    }
                }
                break;
                
            // ========== YEAR ==========
            case 'L': // Leap year
                {
                    int year = timeinfo->tm_year + 1900;
                    int leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
                    if (leap)
                    {
                        result[pos++] = '1';
                    }
                    else
                    {
                        result[pos++] = '0';
                    }
                    result[pos] = '\0';
                }
                break;
                
            case 'Y': // 4-digit year
                sprintf(temp, "%04d", timeinfo->tm_year + 1900);
                strcpy(result + pos, temp);
                pos += 4;
                break;
                
            case 'y': // 2-digit year
                sprintf(temp, "%02d", (timeinfo->tm_year + 1900) % 100);
                strcpy(result + pos, temp);
                pos += 2;
                break;
                
            // ========== TIME ==========
            case 'a': // am/pm lowercase
                if (timeinfo->tm_hour < 12)
                {
                    result[pos++] = 'a';
                    result[pos++] = 'm';
                }
                else
                {
                    result[pos++] = 'p';
                    result[pos++] = 'm';
                }
                result[pos] = '\0';
                break;
                
            case 'A': // AM/PM uppercase
                if (timeinfo->tm_hour < 12)
                {
                    result[pos++] = 'A';
                    result[pos++] = 'M';
                }
                else
                {
                    result[pos++] = 'P';
                    result[pos++] = 'M';
                }
                result[pos] = '\0';
                break;
                
            case 'g': // 12-hour without leading zero
                {
                    int hour = timeinfo->tm_hour % 12;
                    if (hour == 0) hour = 12;
                    sprintf(temp, "%d", hour);
                    strcpy(result + pos, temp);
                    pos += strlen(temp);
                }
                break;
                
            case 'G': // 24-hour without leading zero
                sprintf(temp, "%d", timeinfo->tm_hour);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 'h': // 12-hour with leading zero
                {
                    int hour = timeinfo->tm_hour % 12;
                    if (hour == 0) hour = 12;
                    sprintf(temp, "%02d", hour);
                    strcpy(result + pos, temp);
                    pos += 2;
                }
                break;
                
            case 'H': // 24-hour with leading zero
                sprintf(temp, "%02d", timeinfo->tm_hour);
                strcpy(result + pos, temp);
                pos += 2;
                break;
                
            case 'i': // Minutes
                sprintf(temp, "%02d", timeinfo->tm_min);
                strcpy(result + pos, temp);
                pos += 2;
                break;
                
            case 's': // Seconds
                sprintf(temp, "%02d", timeinfo->tm_sec);
                strcpy(result + pos, temp);
                pos += 2;
                break;
                
            // ========== FULL DATE/TIME ==========
            case 'c': // ISO 8601
                sprintf(temp, "%04d-%02d-%02dT%02d:%02d:%02d+00:00",
                    timeinfo->tm_year + 1900, timeinfo->tm_mon + 1, timeinfo->tm_mday,
                    timeinfo->tm_hour, timeinfo->tm_min, timeinfo->tm_sec);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            case 'r': // RFC 2822
                {
                    const char* days[] = {"Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"};
                    const char* months[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
                    int wday = timeinfo->tm_wday;
                    if (wday == 0) wday = 7;
                    sprintf(temp, "%s, %02d %s %04d %02d:%02d:%02d +0000",
                        days[wday - 1], timeinfo->tm_mday, months[timeinfo->tm_mon],
                        timeinfo->tm_year + 1900, timeinfo->tm_hour, timeinfo->tm_min, timeinfo->tm_sec);
                    strcpy(result + pos, temp);
                    pos += strlen(temp);
                }
                break;
                
            case 'U': // Unix timestamp
                sprintf(temp, "%ld", rawtime);
                strcpy(result + pos, temp);
                pos += strlen(temp);
                break;
                
            // ========== DEFAULT ==========
            default:
                result[pos++] = format[i];
                break;
        }
        i++;
    }
    
    result[pos] = '\0';
    
    // Return the allocated buffer
    return result;
}

// Basic math functions
double sin_f(double x) {
    return sin(x);
}

double cos_f(double x) {
    return cos(x);
}

double tan_f(double x) {
    return tan(x);
}

double asin_f(double x) {
    return asin(x);
}

double acos_f(double x) {
    return acos(x);
}

double atan_f(double x) {
    return atan(x);
}

double atan2_f(double y, double x) {
    return atan2(y, x);
}

// Reciprocal trigonometric functions
double csc_f(double x) {
    return 1.0 / sin(x);
}

double sec_f(double x) {
    return 1.0 / cos(x);
}

double cot_f(double x) {
    return 1.0 / tan(x);
}

// Hyperbolic functions
double sinh_f(double x) {
    return sinh(x);
}

double cosh_f(double x) {
    return cosh(x);
}

double tanh_f(double x) {
    return tanh(x);
}

double asinh_f(double x) {
    return asinh(x);
}

double acosh_f(double x) {
    return acosh(x);
}

double atanh_f(double x) {
    return atanh(x);
}

// Exponential and logarithmic functions
double exp_f(double x) {
    return exp(x);
}

double log_f(double x) {
    return log(x);
}

double log10_f(double x) {
    return log10(x);
}

double log2_f(double x) {
    return log2(x);
}

// Power and root functions
double pow_f(double base, double exp) {
    return pow(base, exp);
}

double sqrt_f(double x) {
    return sqrt(x);
}

double cbrt_f(double x) {
    return cbrt(x);
}

double hypot_f(double x, double y) {
    return hypot(x, y);
}

// Absolute value functions
double abs_f(double x) {
    return fabs(x);
}

// Rounding functions
double ceil_f(double x) {
    return ceil(x);
}

double floor_f(double x) {
    return floor(x);
}

double round_f(double x) {
    return round(x);
}

double trunc_f(double x) {
    return trunc(x);
}

// Error function
double erf_f(double x) {
    return erf(x);
}

double erfc_f(double x) {
    return erfc(x);
}

// Gamma function
double gamma_f(double x) {
    return tgamma(x);
}

// Constant functions
double pi_f() {
    return M_PI;
}

double e_f() {
    return M_E;
}

// Simple regex test - returns 1 if match, 0 if no match, -1 on error
int regex_test(const char* pattern, const char* text, int flags) {
    regex_t regex;
    int ret;
    
    ret = regcomp(&regex, pattern, flags);
    if (ret != 0) {
        return -1;
    }
    
    ret = regexec(&regex, text, 0, NULL, 0);
    regfree(&regex);
    
    if (ret == 0) return 1;
    if (ret == REG_NOMATCH) return 0;
    return -1;
}

// Find first match (group 0 = full match)
char* regex_find(const char* pattern, const char* text, int group, int flags) {
    regex_t regex;
    int ret;
    
    ret = regcomp(&regex, pattern, flags);
    if (ret != 0) {
        return NULL;
    }
    
    regmatch_t matches[10];
    ret = regexec(&regex, text, 10, matches, 0);
    
    if (ret == 0 && group >= 0 && group < 10 && matches[group].rm_so != -1) {
        int start = matches[group].rm_so;
        int end = matches[group].rm_eo;
        int len = end - start;
        
        char* result = malloc(len + 1);
        if (result) {
            strncpy(result, text + start, len);
            result[len] = '\0';
        }
        regfree(&regex);
        return result;
    }
    
    regfree(&regex);
    return NULL;
}

// Replace all occurrences
char* regex_replace(const char* pattern, const char* text, const char* replacement, int flags) {
    regex_t regex;
    int ret;
    
    ret = regcomp(&regex, pattern, flags);
    if (ret != 0) {
        return strdup(text);
    }
    
    // Initial buffer
    size_t text_len = strlen(text);
    size_t replace_len = strlen(replacement);
    char* result = malloc(text_len * 2 + 1);
    if (!result) {
        regfree(&regex);
        return strdup(text);
    }
    
    size_t pos = 0;
    const char* current = text;
    
    while (1) {
        regmatch_t match;
        ret = regexec(&regex, current, 1, &match, 0);
        
        if (ret != 0 || match.rm_so == -1) {
            strcpy(result + pos, current);
            break;
        }
        
        // Copy before match
        int prefix_len = match.rm_so;
        memcpy(result + pos, current, prefix_len);
        pos += prefix_len;
        
        // Copy replacement
        memcpy(result + pos, replacement, replace_len);
        pos += replace_len;
        
        // Move past match
        current += match.rm_eo;
        
        if (*current == '\0') break;
    }
    
    regfree(&regex);
    return result;
}

// Split string by pattern
char** regex_split(const char* pattern, const char* text, int flags, int* count) {
    regex_t regex;
    int ret;
    
    ret = regcomp(&regex, pattern, flags);
    if (ret != 0) {
        *count = 1;
        char** result = malloc(sizeof(char*));
        if (result) result[0] = strdup(text);
        return result;
    }
    
    // Count splits
    int split_count = 1;
    const char* current = text;
    
    while (1) {
        regmatch_t match;
        ret = regexec(&regex, current, 1, &match, 0);
        if (ret != 0 || match.rm_so == -1) break;
        
        split_count++;
        current += match.rm_eo;
        if (*current == '\0') break;
    }
    
    // Allocate results
    char** results = malloc(split_count * sizeof(char*));
    if (!results) {
        regfree(&regex);
        *count = 1;
        char** fallback = malloc(sizeof(char*));
        if (fallback) fallback[0] = strdup(text);
        return fallback;
    }
    
    // Split
    int idx = 0;
    current = text;
    
    while (1) {
        regmatch_t match;
        ret = regexec(&regex, current, 1, &match, 0);
        
        if (ret != 0 || match.rm_so == -1) {
            results[idx] = strdup(current);
            break;
        }
        
        int part_len = match.rm_so;
        results[idx] = malloc(part_len + 1);
        if (results[idx]) {
            strncpy(results[idx], current, part_len);
            results[idx][part_len] = '\0';
        } else {
            results[idx] = strdup("");
        }
        idx++;
        
        current += match.rm_eo;
        if (*current == '\0') {
            results[idx] = strdup("");
            idx++;
            break;
        }
    }
    
    *count = idx;
    regfree(&regex);
    return results;
}

// Free split results
void free_split_results(char** results, int count) {
    if (results) {
        for (int i = 0; i < count; i++) {
            if (results[i]) free(results[i]);
        }
        free(results);
    }
}

// Free single string
void free_string(char* ptr)
{
    if (ptr) free(ptr);
}

struct ResponseData
{
    char* data;
    size_t size;
};

// Callback function for curl
size_t write_callback(void* contents, size_t size, size_t nmemb, void* userp)
{
    size_t real_size = size * nmemb;
    struct ResponseData* mem = (struct ResponseData*)userp;
    
    char* ptr = realloc(mem->data, mem->size + real_size + 1);
    if (!ptr) return 0;
    
    mem->data = ptr;
    memcpy(&(mem->data[mem->size]), contents, real_size);
    mem->size += real_size;
    mem->data[mem->size] = 0;
    
    return real_size;
}

// HTTP GET request
char* http_get(const char* url)
{
    CURL* curl;
    CURLcode res;
    struct ResponseData response;
    
    response.data = malloc(1);
    response.size = 0;
    
    curl = curl_easy_init();
    if (!curl)
    {
        free(response.data);
        return NULL;
    }
    
    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, (void*)&response);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, 
        "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0");

    res = curl_easy_perform(curl);
    
    if (res != CURLE_OK)
    {
        curl_easy_cleanup(curl);
        free(response.data);
        return NULL;
    }
    
    curl_easy_cleanup(curl);
    return response.data;
}

// HTTP POST request with data
char* http_post(const char* url, const char* post_data)
{
    CURL* curl;
    CURLcode res;
    struct ResponseData response;
    
    response.data = malloc(1);
    response.size = 0;
    
    curl = curl_easy_init();
    if (!curl)
    {
        free(response.data);
        return NULL;
    }
    
    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_POSTFIELDS, post_data);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, (void*)&response);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, 
        "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0");

    
    res = curl_easy_perform(curl);
    
    if (res != CURLE_OK)
    {
        curl_easy_cleanup(curl);
        free(response.data);
        return NULL;
    }
    
    curl_easy_cleanup(curl);
    return response.data;
}

// HTTP request with custom method and headers
char* http_request(const char* method, const char* url, const char* headers, const char* body)
{
    CURL* curl;
    CURLcode res;
    struct ResponseData response;
    struct curl_slist* header_list = NULL;
    
    response.data = malloc(1);
    response.size = 0;
    
    curl = curl_easy_init();
    if (!curl)
    {
        free(response.data);
        return NULL;
    }
    
    // Set method
    if (strcmp(method, "GET") == 0)
    {
        curl_easy_setopt(curl, CURLOPT_HTTPGET, 1L);
    } 
    else if (strcmp(method, "POST") == 0)
    {
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
        if (body) curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body);
    }
    else if (strcmp(method, "PUT") == 0)
    {
        curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "PUT");
        if (body) curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body);
    }
    else if (strcmp(method, "DELETE") == 0)
    {
        curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "DELETE");
    }
    
    header_list = curl_slist_append(header_list, 
        "User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0");
    
    // Set custom headers if provided
    if (headers && strlen(headers) > 0)
    {
        char* headers_copy = strdup(headers);
        char* token = strtok(headers_copy, "\n");
        while (token)
        {
            header_list = curl_slist_append(header_list, token);
            token = strtok(NULL, "\n");
        }
        free(headers_copy);
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }
    else
    {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, header_list);
    }
    
    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, (void*)&response);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    
    res = curl_easy_perform(curl);
    
    if (header_list) curl_slist_free_all(header_list);
    
    if (res != CURLE_OK)
    {
        curl_easy_cleanup(curl);
        free(response.data);
        return NULL;
    }
    
    curl_easy_cleanup(curl);
    return response.data;
}

char* json_encode(const char* input) {
    if (!input) {
        char* result = malloc(5);
        if (result) strcpy(result, "null");
        return result;
    }
    
    // Check if it's already a JSON object or array
    const char* trimmed = input;
    while (*trimmed && isspace(*trimmed)) trimmed++;
    if ((*trimmed == '{' || *trimmed == '[') && 
        (trimmed[strlen(trimmed)-1] == '}' || trimmed[strlen(trimmed)-1] == ']')) {
        char* result = malloc(strlen(input) + 1);
        if (result) strcpy(result, input);
        return result;
    }
    
    // Check if it's a number
    bool is_number = true;
    bool has_dot = false;
    const char* p = input;
    if (*p == '-') p++;
    while (*p) {
        if (*p == '.') {
            if (has_dot) { is_number = false; break; }
            has_dot = true;
            p++;
            continue;
        }
        if (!isdigit(*p)) {
            is_number = false;
            break;
        }
        p++;
    }
    
    if (is_number && strlen(input) > 0) {
        char* result = malloc(strlen(input) + 1);
        if (result) strcpy(result, input);
        return result;
    }
    
    // Check if it's boolean or null
    if (strcmp(input, "true") == 0 || strcmp(input, "false") == 0 || strcmp(input, "null") == 0) {
        char* result = malloc(strlen(input) + 1);
        if (result) strcpy(result, input);
        return result;
    }
    
    // For arrays: check if input looks like an array
    if (*trimmed == '[') {
        char* result = malloc(strlen(input) + 1);
        if (result) strcpy(result, input);
        return result;
    }
    
    // For objects: check if input looks like an object
    if (*trimmed == '{') {
        char* result = malloc(strlen(input) + 1);
        if (result) strcpy(result, input);
        return result;
    }
    
    // Escape and quote string (only for plain strings)
    int len = strlen(input);
    int escaped_len = 1; // for null terminator
    for (int i = 0; i < len; i++) {
        char c = input[i];
        if (c == '"' || c == '\\' || c == '/' || c == '\b' || c == '\f' || c == '\n' || c == '\r' || c == '\t') {
            escaped_len += 2;
        } else {
            escaped_len += 1;
        }
    }
    
    char* result = malloc(escaped_len + 2); // +2 for quotes
    if (!result) return NULL;
    
    int pos = 0;
    result[pos++] = '"';
    for (int i = 0; i < len; i++) {
        char c = input[i];
        switch (c) {
            case '"':  result[pos++] = '\\'; result[pos++] = '"'; break;
            case '\\': result[pos++] = '\\'; result[pos++] = '\\'; break;
            case '/':  result[pos++] = '\\'; result[pos++] = '/'; break;
            case '\b': result[pos++] = '\\'; result[pos++] = 'b'; break;
            case '\f': result[pos++] = '\\'; result[pos++] = 'f'; break;
            case '\n': result[pos++] = '\\'; result[pos++] = 'n'; break;
            case '\r': result[pos++] = '\\'; result[pos++] = 'r'; break;
            case '\t': result[pos++] = '\\'; result[pos++] = 't'; break;
            default:   result[pos++] = c; break;
        }
    }
    result[pos++] = '"';
    result[pos] = '\0';
    
    return result;
}

char* json_decode(const char* input) {
    if (!input) {
        char* result = malloc(5);
        if (result) strcpy(result, "null");
        return result;
    }
    
    // Skip whitespace
    while (*input && isspace(*input)) input++;
    
    if (!*input) {
        char* result = malloc(5);
        if (result) strcpy(result, "null");
        return result;
    }
    
    // Handle null
    if (strncmp(input, "null", 4) == 0) {
        char* result = malloc(5);
        if (result) strcpy(result, "null");
        return result;
    }
    
    // Handle true
    if (strncmp(input, "true", 4) == 0) {
        char* result = malloc(5);
        if (result) strcpy(result, "true");
        return result;
    }
    
    // Handle false
    if (strncmp(input, "false", 5) == 0) {
        char* result = malloc(6);
        if (result) strcpy(result, "false");
        return result;
    }
    
    // Handle number
    bool is_number = true;
    bool has_dot = false;
    const char* temp = input;
    if (*temp == '-') temp++;
    while (*temp && !isspace(*temp) && *temp != ',' && *temp != '}' && *temp != ']') {
        if (*temp == '.') {
            if (has_dot) { is_number = false; break; }
            has_dot = true;
        } else if (!isdigit(*temp)) {
            is_number = false;
            break;
        }
        temp++;
    }
    
    if (is_number && input != temp) {
        int len = temp - input;
        char* result = malloc(len + 1);
        if (result) {
            strncpy(result, input, len);
            result[len] = '\0';
        }
        return result;
    }
    
    // Handle string
    if (*input == '"') {
        input++;
        const char* start = input;
        int len = 0;
        bool escaped = false;
        
        while (*input && (*input != '"' || escaped)) {
            if (escaped) {
                escaped = false;
            } else if (*input == '\\') {
                escaped = true;
            }
            len++;
            input++;
        }
        
        if (*input == '"') {
            char* result = malloc(len + 1);
            if (result) {
                int pos = 0;
                for (int i = 0; i < len; i++) {
                    if (start[i] == '\\' && i + 1 < len) {
                        char c = start[++i];
                        switch (c) {
                            case '"':  result[pos++] = '"'; break;
                            case '\\': result[pos++] = '\\'; break;
                            case '/':  result[pos++] = '/'; break;
                            case 'b':  result[pos++] = '\b'; break;
                            case 'f':  result[pos++] = '\f'; break;
                            case 'n':  result[pos++] = '\n'; break;
                            case 'r':  result[pos++] = '\r'; break;
                            case 't':  result[pos++] = '\t'; break;
                            default:   result[pos++] = c; break;
                        }
                    } else {
                        result[pos++] = start[i];
                    }
                }
                result[pos] = '\0';
            }
            return result;
        }
    }
    
    // Handle array - return as string
    if (*input == '[') {
        const char* start = input;
        int depth = 1;
        while (*input && depth > 0) {
            input++;
            if (*input == '[') depth++;
            if (*input == ']') depth--;
        }
        if (*input == ']') {
            input++;
            int len = input - start;
            char* result = malloc(len + 1);
            if (result) {
                strncpy(result, start, len);
                result[len] = '\0';
            }
            return result;
        }
    }
    
    // Handle object - return as string
    if (*input == '{') {
        const char* start = input;
        int depth = 1;
        while (*input && depth > 0) {
            input++;
            if (*input == '{') depth++;
            if (*input == '}') depth--;
        }
        if (*input == '}') {
            input++;
            int len = input - start;
            char* result = malloc(len + 1);
            if (result) {
                strncpy(result, start, len);
                result[len] = '\0';
            }
            return result;
        }
    }
    
    // Fallback: return input as string
    char* result = malloc(strlen(input) + 3);
    if (result) sprintf(result, "\"%s\"", input);
    return result;
}

char* range(int start, int end, int step)
{
    // Validate step
    if (step == 0)
    {
        char* result = malloc(7);
        if (result) strcpy(result, "[null]");
        return result;
    }
    
    // Calculate number of elements
    int count = 0;
    if (step > 0)
    {
        if (start > end)
        {
            char* result = malloc(7);
            if (result) strcpy(result, "[null]");
            return result;
        }
        count = (end - start) / step + 1;
    }
    else
    {
        if (start < end)
        {
            char* result = malloc(7);
            if (result) strcpy(result, "[null]");
            return result;
        }
        count = (start - end) / (-step) + 1;
    }
    
    if (count <= 0)
    {
        char* result = malloc(7);
        if (result) strcpy(result, "[null]");
        return result;
    }
    
    // Calculate buffer size
    // Each number: up to 20 chars + ", " (2 chars) + brackets and null
    int buffer_size = 3; // for "[" and "]"
    for (int i = 0; i < count; i++)
    {
        int num = start + i * step, digits = 1;
        int temp = num;
        if (temp < 0)
        {
            digits++;
            temp = -temp;
        }
        while (temp >= 10)
        {
            digits++;
            temp /= 10;
         }
        buffer_size += digits;
        if (i < count - 1) buffer_size += 2; // ", "
    }
    buffer_size += 1; // null terminator
    
    char* result = malloc(buffer_size);
    if (!result) return NULL;
    
    // Build the string
    int pos = 0;
    result[pos++] = '[';
    
    for (int i = 0; i < count; i++)
    {
        int num = start + i * step;
        char num_str[32];
        sprintf(num_str, "%d", num);
        
        for (int j = 0; num_str[j] != '\0'; j++) result[pos++] = num_str[j];
        
        if (i < count - 1)
        {
            result[pos++] = ',';
            result[pos++] = ' ';
        }
    }
    
    result[pos++] = ']';
    result[pos] = '\0';
    
    return result;
}

char* uuid_v4(void)
{
    static int seeded = 0;
    if (!seeded)
    {
        srand((unsigned int)time(NULL));
        seeded = 1;
    }
    
    char* uuid = calloc(37, sizeof(char));
    if (!uuid) return NULL;
    
    unsigned char bytes[16];
    for (int i = 0; i < 16; i++) bytes[i] = rand() & 0xFF;
    
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    
    sprintf(uuid, 
            "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            bytes[6], bytes[7],
            bytes[8], bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]);
    return uuid;
}

int strcmp_prism(const char* a, const char* b)
{
    return strcmp(a, b);
} 


int download_to_file(const char* url, const char* output_path)
{
    CURL *curl_handle;
    CURLcode res;
    FILE *file;
    
    file = fopen(output_path, "wb");
    if (!file)
    {
        fprintf(stderr, "download_to_file: failed to create %s\n", output_path);
        return EXIT_FAILURE;
    }
    
    curl_global_init(CURL_GLOBAL_ALL);
    curl_handle = curl_easy_init();
    
    if (!curl_handle)
    {
        fclose(file);
        curl_global_cleanup();
        return EXIT_FAILURE;
    }
    
    curl_easy_setopt(curl_handle, CURLOPT_URL, url);
    curl_easy_setopt(curl_handle, CURLOPT_WRITEFUNCTION, NULL);
    curl_easy_setopt(curl_handle, CURLOPT_WRITEDATA, file);
    curl_easy_setopt(curl_handle, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl_handle, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl_handle, CURLOPT_USERAGENT, "Prism-Package-Manager/1.0");
    curl_easy_setopt(curl_handle, CURLOPT_SSL_VERIFYPEER, 1L);
    curl_easy_setopt(curl_handle, CURLOPT_SSL_VERIFYHOST, 2L);
    
    res = curl_easy_perform(curl_handle);
    
    fclose(file);
    curl_easy_cleanup(curl_handle);
    curl_global_cleanup();
    
    if (res != CURLE_OK)
    {
        fprintf(stderr, "download_to_file: curl failed: %s\n", curl_easy_strerror(res));
        remove(output_path);
        return EXIT_FAILURE;
    }
    
    return 0;
}

int unzip_file(const char* zip_path, const char* dest_dir)
{
    int err = 0;
    
    zip_t *zip = zip_open(zip_path, ZIP_RDONLY, &err);
    if (!zip)
    {
        fprintf(stderr, "unzip_file: failed to open zip: %s (error code: %d)\n", zip_path, err);
        return -1;
    }
    
    mkdir(dest_dir, 0755);
    zip_int64_t num_entries = zip_get_num_entries(zip, 0);
    if (num_entries < 0)
    {
        fprintf(stderr, "unzip_file: zip_get_num_entries failed\n");
        zip_close(zip);
        return -1;
    }

    for (zip_int64_t i = 0; i < num_entries; i++)
    {
        struct zip_stat st;
        zip_stat_init(&st);
        
        if (zip_stat_index(zip, i, 0, &st) != 0)
        {
            fprintf(stderr, "unzip_file: zip_stat_index failed for entry %lld\n", (long long)i);
            continue;
        }
        
        const char* name = st.name;
        if (!name) continue;
        
        char full_path[2048];
        snprintf(full_path, sizeof(full_path), "%s/%s", dest_dir, name);
        
        size_t name_len = strlen(name);
        if (name[name_len - 1] == '/')
        {
            mkdir(full_path, 0755);
            continue;
        }
        
        char* last_slash = strrchr(full_path, '/');
        if (last_slash)
        {
            *last_slash = '\0';
            mkdir(full_path, 0755);
            *last_slash = '/';
        }

        zip_file_t *zf = zip_fopen_index(zip, i, 0);
        if (!zf)
        {
            fprintf(stderr, "unzip_file: zip_fopen_index failed for %s\n", name);
            continue;
        }
        
        FILE *f = fopen(full_path, "wb");
        if (!f)
        {
            fprintf(stderr, "unzip_file: failed to create %s\n", full_path);
            zip_fclose(zf);
            continue;
        }
        
        char buf[8192];
        zip_int64_t len;
        while ((len = zip_fread(zf, buf, sizeof(buf))) > 0) fwrite(buf, 1, len, f);
        
        fclose(f);
        zip_fclose(zf);
    }
    
    zip_close(zip);
    return 0;
}