async function main() {
    try {
        const res = await fetch("https://appropriate-chelsea-mist-labs-1f0a1134.koyeb.app/api/v1/bridge/intents?status=pending&limit=1");
        const json = await res.json();
        console.log(JSON.stringify(json, null, 2));
    } catch(e) {
        console.error(e);
    }
}
main();
