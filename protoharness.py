import requests
import json
from typing import List, Dict, Any

# ============================================
# 1. CONFIGURATION ET PARAMÈTRES (Config)
# ============================================

class AgentConfig:
    """
    Gestionnaire de configuration minimaliste pour les connexions LLM.
    Tout est centralisé ici pour une maintenance facile et une fiabilité accrue.
    """
    def __init__(self, base_url: str = "http://localhost:11434", default_model: str = "llama3"):
        self.base_url = base_url
        # NOTE: Le modèle est critique pour la performance de l'agent.
        self.default_model = default_model

    def get_ollama_url(self) -> str:
        return f"{self.base_url}/api/generate"


# ============================================
# 2. CLIENT LLM (L'Adaptateur de Performance - OpenClaw Concept)
# ============================================

class OllamaClient:
    """
    Interface robuste pour interagir avec une instance locale d'Ollama.
    Gère l'authentification et le format des requêtes HTTP.
    """
    def __init__(self, config: AgentConfig):
        self.config = config
        print(f"[INIT] Connecté au client Ollama sur {config.base_url}.")

    def generate_response(self, 
                           prompt: str, 
                           model: str) -> str:
        """
        Envoie une requête générique à Ollama et retourne le texte de réponse.
        
        Args:
            prompt: Le prompt détaillé (Planification, Critique, etc.).
            model: Le nom du modèle à utiliser (ex: "llama3").

        Returns:
            La réponse textuelle du modèle LLM.
        """
        url = self.config.get_ollama_url()
        payload = {
            "model": model,
            "prompt": prompt,
            "stream": False,  # Non-streaming pour la simplicité du prototype
            "options": {"temperature": 0.1} # Température basse pour la fiabilité/logique
        }

        try:
            response = requests.post(url, json=payload)
            response.raise_for_status() # Lève une exception pour les codes 4xx/5xx
            
            data = response.json()
            return data.get("response", "").strip()

        except requests.exceptions.ConnectionError:
            print("\n[ERREUR Critique] Impossible de se connecter à Ollama.")
            print("Vérifiez que l'instance Ollama est lancée et que le modèle est pullé.")
            return "EXECUTION_FAILED: Vérifiez la connexion ou le modèle."
        except requests.exceptions.HTTPError as e:
             if response.status_code == 404:
                 return f"ERROR: Le chemin API n'existe pas ou le modèle '{model}' est inconnu."
             print(f"\n[ERREUR HTTP] Erreur {response.status_code}: {e}")
             return "EXECUTION_FAILED: Erreur HTTP de l'API Ollama."
        except Exception as e:
            return f"FATAL ERROR during API call: {e}"


# ============================================
# 3. MOTEUR D'AGENT (AgentCore - Le Loop Engineering)
# ============================================

class AgentCore:
    """
    Le cœur de l'agent. Implémente le cycle critique de "Loop Engineering" :
    PLAN -> EXECUTE -> REFLECT/CRITIQUE.
    Ce moteur orchestre les appels LLM pour atteindre un objectif complexe.
    """
    def __init__(self, client: OllamaClient):
        self.llm_client = client

    def _simulate_tool_execution(self, action: str) -> str:
        """
        Simule l'exécution d'une action externe (un "outil").
        Dans un vrai système, ceci appellerait des API réelles ou du code Python.
        Est le point de rupture entre la logique et l'environnement réel.
        """
        print(f"\n{'='*20} EXÉCUTION D'ACTION : {action} {'='*20}")
        if "rechercher" in action:
            return f"Observation: Des résultats de recherche pour '{action}' ont été trouvés. Le point clé est X."
        elif "calculer" in action:
            # Simule un calcul réussi
            return f"Observation: L'opération mathématique demandée a abouti à la valeur 42."
        else:
            return f"Observation: Action '{action}' exécutée avec succès, mais aucune donnée spécifique n'a été capturée."

    def run_agent(self, user_goal: str) -> str:
        """
        Le cycle principal de l'Agent. 
        Utilise un prompt structuré pour forcer la pensée étape par étape (CoT).
        """
        print("\n" + "="*80)
        print("🚀 DÉBUT DU CYCLE AGENT INTÉGRÉ : PLAN -> EXECUTE -> REFLECT")
        print(f"Objectif utilisateur: {user_goal}")
        print("="*80 + "\n")

        # --- Étape 1: Planification (PLAN) ---
        plan_prompt = f"""
        Tu es un agent d'exécution hautement fiable. L'objectif est le suivant : "{user_goal}".
        Avant de répondre, tu dois nécessairement décomposer cet objectif en une série d'étapes logiques et concrètes actions à effectuer. 
        Ne réponds qu'avec un JSON valide contenant une liste nommée "steps". Chaque étape doit être une chaîne décrivant l'action (ex: 'rechercher le prix de l\'euro') ou la fonction à appeler.
        Exemple de réponse attendue : {{"steps": ["Action 1", "Action 2"]}}
        """
        print("[PHASE 1/3] 🧠 Génération du Planification...")
        plan_response = self.llm_client.generate_response(plan_prompt, self.llm_client.config.default_model)

        try:
            # Tente de charger le JSON généré par l'LLM
            plan_json = json.loads(plan_response)
            steps: List[str] = plan_json.get("steps", [])
            print(f"✅ Planification réussie. Détecté {len(steps)} étapes : {', '.join(steps)}")
        except (json.JSONDecodeError, KeyError):
            print("\n[ATTENTION] Échec de l'analyse JSON du plan. Traitement des résultats bruts.")
            return f"Erreur critique lors de la planification. Le modèle n'a pas retourné un format JSON valide. Réponse brute reçue: {plan_response}"

        # --- Étape 2: Exécution (EXECUTE) ---
        print("\n" + "="*80)
        print("🏃 PHASE 2/3 : EXÉCUTION DES OUTILS ET ACTIONS")
        observations: List[str] = []

        for i, step in enumerate(steps):
            # Ici on remplace l'appel à un outil réel par une simulation.
            observation = self._simulate_tool_execution(step)
            observations.append(observation)
            print(f"  -> Observation {i+1}: {observation}")

        # --- Étape 3: Réflexion et Finalisation (REFLECT/CRITIQUE) ---
        print("\n" + "="*80)
        print("🧠 PHASE 3/3 : RÉFLEXION ET SYNTHÈSE FINALE")
        
        critique_context = "\n\n--- CONTEXTE D'OBSERVATION ---\n"
        for i, obs in enumerate(observations):
            critique_context += f"Observation {i+1}: {obs}\n"

        reflection_prompt = f"""
        Tu es un agent de synthèse et de critique. L'objectif initial était : "{user_goal}". 
        Nous avons exécuté les actions suivantes, qui ont généré les observations ci-dessous. 
        Ton rôle est double : 
        1. Analyser si toutes les informations nécessaires pour atteindre l'objectif sont présentes. 
        2. Produire la réponse finale et synthétique pour l'utilisateur. 
        N'ajoute aucun autre commentaire, juste une conclusion fluide et professionnelle.

        OBSERVATIONS COMPLÈTES:
        {critique_context}
        """

        final_response = self.llm_client.generate_response(reflection_prompt, self.llm_client.config.default_model)
        return final_response


# ============================================
# 💻 EXÉCUTION PRINCIPALE ET TEST
# ============================================

if __name__ == "__main__":
    try:
        # Initialisation avec le modèle par défaut (assurez-vous qu'il existe localement)
        config = AgentConfig(default_model="llama3") 
        client = OllamaClient(config=config)
        agent = AgentCore(client=client)

        user_goal_1 = "Détermine la meilleure stratégie d'investissement pour un petit capital. Tu dois commencer par chercher des tendances économiques et calculer le potentiel de croissance."
        
        print("\n##############################################################")
        print("### TEST 1: SCÉNARIO COMPOSÉ (Planification, Tools, Critique) ###")
        final_result = agent.run_agent(user_goal_1)

        print("\n" + "#"*80)
        print("✨ RÉSULTAT FINAL DE L'AGENT ✨".center(80))
        print("-" * 30)
        print(final_result)
        print("#"*80)


    except Exception as e:
        print(f"\n[ERREUR FATALE] Une exception inattendue est survenue : {e}")

