import kotlinx.coroutines.runBlocking
import uniffi.goose_llm.*

fun main() = runBlocking {
    // Initialize the provider pool (optional, but good practice)
    initProviderPool()
    
    // Configure the provider pool with custom settings
    configureProviderPool(
        maxSize = 10,     // Max 10 connections in the pool
        maxIdleSec = 300, // 5 minutes max idle time
        maxLifetimeSec = 3600, // 1 hour max lifetime
        maxUses = 100     // Max 100 uses per connection
    )
    val now = System.currentTimeMillis() / 1000
    val msgs = listOf(
        // 1) User sends a plain-text prompt
        Message(
            role    = Role.USER,
            created = now,
            content = listOf(
                MessageContent.Text(
                    TextContent("What is 7 x 6?")
                )
            )
        ),

        // 2) Assistant makes a tool request (ToolReq) to calculate 7×6
        Message(
            role    = Role.ASSISTANT,
            created = now + 2,
            content = listOf(
                MessageContent.ToolReq(
                    ToolRequest(
                        id = "calc1",
                        toolCall = """
                            {
                              "status": "success",
                              "value": {
                                "name": "calculator_extension__toolname",
                                "arguments": {
                                  "operation": "multiply",
                                  "numbers": [7, 6]
                                }, 
                                "needsApproval": false
                              }                              
                            }
                        """.trimIndent()
                    )
                )
            )
        ),

        // 3) User (on behalf of the tool) responds with the tool result (ToolResp)
        Message(
            role    = Role.USER,
            created = now + 3,
            content = listOf(
                MessageContent.ToolResp(
                    ToolResponse(
                        id = "calc1",
                        toolResult = """
                            {
                              "status": "success",
                              "value": [
                                {"type": "text", "text": "42"}
                              ]                        
                            }
                        """.trimIndent()
                    )
                )
            )
        ), 
    )

    printMessages(msgs)
    println("---\n")

    // Setup provider
    val providerName = "databricks"
    val host = System.getenv("DATABRICKS_HOST") ?: error("DATABRICKS_HOST not set")
    val token = System.getenv("DATABRICKS_TOKEN") ?: error("DATABRICKS_TOKEN not set")
    val providerConfig = """{"host": "$host", "token": "$token"}"""

    println("Provider Name: $providerName")
    println("Provider Config: $providerConfig")


    val sessionName = generateSessionName(providerName, providerConfig, msgs)
    println("\nSession Name: $sessionName")

    val tooltip = generateTooltip(providerName, providerConfig, msgs)
    println("\nTooltip: $tooltip")

    // Completion
    val modelName = "goose-gpt-4-1"
    val modelConfig = ModelConfig(
        modelName,
        100000u,  // UInt
        0.1f,     // Float
        200      // Int
    )

    val calculatorTool = createToolConfig(
        name = "calculator",
        description = "Perform basic arithmetic operations",
        inputSchema = """
            {
                "type": "object",
                "required": ["operation", "numbers"],
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["add", "subtract", "multiply", "divide"],
                        "description": "The arithmetic operation to perform"
                    },
                    "numbers": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "List of numbers to operate on in order"
                    }
                }
            }
        """.trimIndent(),
        approvalMode = ToolApprovalMode.AUTO
    )

    val calculator_extension = ExtensionConfig(
        name = "calculator_extension",
        instructions = "This extension provides a calculator tool.",
        tools = listOf(calculatorTool)
    )

    val extensions = listOf(calculator_extension)
    val systemPreamble = "You are a helpful assistant."


    val req = createCompletionRequest(
        providerName,
        providerConfig,
        modelConfig,
        systemPreamble,
        msgs,
        extensions,
        usePool = true // Enable connection pooling
    )

    val response = completion(req)
    println("\nCompletion Response:\n${response.message}")
    println()

    // ---- UI Extraction (custom schema) ----
    runUiExtraction(providerName, providerConfig)
}


suspend fun runUiExtraction(providerName: String, providerConfig: String) {
    val systemPrompt = "You are a UI generator AI. Convert the user input into a JSON-driven UI."
    val messageText = """
    [
  {
    "year": 2015,
    "unique_artists": 71
  },
  {
    "year": 2016,
    "unique_artists": 51
  },
  {
    "year": 2017,
    "unique_artists": 121
  },
  {
    "year": 2018,
    "unique_artists": 92
  },
  {
    "year": 2019,
    "unique_artists": 377
  },
  {
    "year": 2020,
    "unique_artists": 335
  },
  {
    "year": 2021,
    "unique_artists": 383
  },
  {
    "year": 2022,
    "unique_artists": 444
  },
  {
    "year": 2023,
    "unique_artists": 510
  },
  {
    "year": 2024,
    "unique_artists": 627
  },
  {
    "year": 2025,
    "unique_artists": 243
  }
]
""".trimIndent()

    val messages = listOf(
        Message(
            role = Role.USER,
            created = System.currentTimeMillis() / 1000,
            content = listOf(
                MessageContent.Text(
                    TextContent(messageText)
                )
            )
        )
    )

    val schema2 = """
    {
  "type": "object",
  "properties": {
    "chartType": {
      "const": "line",
      "type": "string"
    },
    "xAxis": {
      "type": "array",
      "items": { "type": "number" }
    },
    "yAxis": {
      "type": "array",
      "items": { "type": "number" }
    }
  },
  "required": ["chartType", "xAxis", "yAxis"],
  "additionalProperties": false
}
""".trimIndent();

    val schema = """{
  "properties": {
    "chartType": {
      "const": "line",
      "type": "string"
    },
    "xAxis": {
      "title": "Year",
      "type": "string"
    },
    "yAxis": {
      "title": "Number of Unique Artists",
      "type": "number"
    }
  },
  "type": "object",
   "additionalProperties": false,
    "required": ["chartType", "xAxis", "yAxis"]
    }""".trimIndent();

    try {
        val response = generateStructuredOutputs(
            providerName = providerName,
            providerConfig = providerConfig,
            systemPrompt = systemPrompt,
            messages = messages,
            schema = schema2
        )
        println("\nUI Extraction Output:\n${response}")
    } catch (e: ProviderException) {
        println("\nUI Extraction failed:\n${e.message}")
    }
}
